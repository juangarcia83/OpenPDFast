// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tile scheduler: turns viewports into prioritized tile jobs, runs them on a
//! rayon pool, cancels obsolete work and caches results.
//!
//! - One *preparer* thread interprets pages into display lists (documents are
//!   not thread-safe), most urgent page first.
//! - Rasterization runs on the pool. Every queued job is paired with one pool
//!   task that pops the *currently* most urgent job when it runs, so priority
//!   is respected even though rayon itself is FIFO/LIFO.
//! - A new viewport replaces the wanted set: stale queued jobs are skipped and
//!   in-flight renders that are no longer wanted are aborted via their cookie.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use std::thread;

use parking_lot::{Condvar, Mutex};
use serde::Deserialize;
use tracing::{debug, info_span, warn};

use crate::cache::CostLru;
use crate::renderer::{CancelToken, MeasuredSizes, PageRenderer, RenderError, RgbaImage};
use crate::tile::{PageRect, TileKey, ZoomLevel, tile_rect, tiles_in_rect};

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Rasterization threads (ADR 0002: MuPDF scales to ~4 on heavy pages).
    pub threads: usize,
    /// Memory budget of the tile cache, in bytes.
    pub tile_budget: usize,
    /// How many prepared pages (display lists) to keep. MuPDF does not expose
    /// their size, so this budget counts pages.
    pub prepared_pages: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        let cores = thread::available_parallelism().map_or(2, usize::from);
        Self {
            threads: cores.clamp(1, 4),
            tile_budget: 768 << 20,
            prepared_pages: 64,
        }
    }
}

/// Part of a page, in points, that the frontend wants rendered at `level`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PageRegion {
    pub page: u32,
    pub rect: PageRect,
    pub level: ZoomLevel,
}

/// What the frontend currently shows. The backend decides which tiles that
/// needs; the frontend only describes geometry (AGENTS.md §4).
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Viewport {
    /// On-screen regions, most important first.
    pub visible: Vec<PageRegion>,
    /// Off-screen regions likely to be shown next (in scroll direction first),
    /// typically at a lower level so they are cheap to prefetch.
    pub nearby: Vec<PageRegion>,
    /// Tiles the frontend dropped since the last viewport; they will be sent
    /// again if needed.
    pub evicted: Vec<TileKey>,
}

#[derive(Debug, Clone)]
pub struct RenderedTile {
    pub key: TileKey,
    pub image: Arc<RgbaImage>,
}

#[derive(Debug, Clone)]
pub enum RenderEvent {
    Tile(RenderedTile),
    PageFailed {
        page: u32,
        message: String,
    },
    /// Exact sizes for pages that were shown with an estimated size.
    PageSizes(MeasuredSizes),
    /// Everything rendered so far is stale (e.g. a layer was toggled): drop
    /// all tiles and send the viewport again.
    Invalidated,
}

pub type Sink = Arc<dyn Fn(RenderEvent) + Send + Sync>;

/// Lower is more urgent: visible before nearby, then by region order, then
/// by distance (in points) to the region's center.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Priority {
    class: u8,
    region: u16,
    dist: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct Job {
    prio: Priority,
    seq: u64,
    key: TileKey,
}

impl Ord for Job {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap pops the maximum: invert so the most urgent comes first.
        other
            .prio
            .cmp(&self.prio)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

impl PartialOrd for Job {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct State<P> {
    generation: u64,
    wanted: HashMap<TileKey, Priority>,
    queue: BinaryHeap<Job>,
    seq: u64,
    in_flight: HashMap<TileKey, CancelToken>,
    delivered: HashSet<TileKey>,
    tiles: CostLru<TileKey, RgbaImage>,
    prepared: CostLru<u32, P>,
    preparing: Option<u32>,
    failed: HashSet<u32>,
    /// All page sizes are exact (nothing left for `measure_more`).
    measured: bool,
    /// Bumped by `invalidate`; work started in an older epoch is discarded.
    epoch: u64,
    closed: bool,
}

impl<P> State<P> {
    fn push(&mut self, key: TileKey, prio: Priority) {
        self.seq += 1;
        self.queue.push(Job {
            prio,
            seq: self.seq,
            key,
        });
    }

    /// Most urgent page that has wanted tiles but no display list yet.
    fn next_page_to_prepare(&self) -> Option<u32> {
        self.wanted
            .iter()
            .filter(|(k, _)| !self.prepared.contains(&k.page) && !self.failed.contains(&k.page))
            .min_by_key(|(_, p)| **p)
            .map(|(k, _)| k.page)
    }
}

struct Shared<R: PageRenderer> {
    renderer: R,
    state: Mutex<State<R::Prepared>>,
    wake_preparer: Condvar,
    pool: rayon::ThreadPool,
    sink: Sink,
}

/// Snapshot for tests and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub wanted: usize,
    pub queued: usize,
    pub in_flight: usize,
    pub preparing: bool,
    pub cached_tiles: usize,
    pub cached_bytes: usize,
}

pub struct Scheduler<R: PageRenderer> {
    shared: Arc<Shared<R>>,
}

impl<R: PageRenderer> Scheduler<R> {
    pub fn new(renderer: R, config: SchedulerConfig, sink: Sink) -> Result<Self, RenderError> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(config.threads.max(1))
            .thread_name(|i| format!("render-{i}"))
            .build()
            .map_err(|e| RenderError::Backend(e.to_string()))?;
        let shared = Arc::new(Shared {
            renderer,
            state: Mutex::new(State {
                generation: 0,
                wanted: HashMap::new(),
                queue: BinaryHeap::new(),
                seq: 0,
                in_flight: HashMap::new(),
                delivered: HashSet::new(),
                tiles: CostLru::new(config.tile_budget),
                prepared: CostLru::new(config.prepared_pages.max(1)),
                preparing: None,
                failed: HashSet::new(),
                measured: false,
                epoch: 0,
                closed: false,
            }),
            wake_preparer: Condvar::new(),
            pool,
            sink,
        });
        let for_thread = Arc::clone(&shared);
        thread::Builder::new()
            .name("render-prepare".into())
            .spawn(move || preparer_loop(&for_thread))
            .map_err(|e| RenderError::Backend(e.to_string()))?;
        Ok(Self { shared })
    }

    pub fn renderer(&self) -> &R {
        &self.shared.renderer
    }

    /// Replaces the set of wanted tiles. Never blocks on rendering.
    pub fn set_viewport(&self, viewport: Viewport) {
        let shared = &self.shared;
        let mut ready = Vec::new();
        let jobs;
        {
            let mut st = shared.state.lock();
            st.generation += 1;
            let _span = info_span!("render.viewport", generation = st.generation).entered();
            for key in &viewport.evicted {
                st.delivered.remove(key);
            }
            let wanted = wanted_tiles(&shared.renderer, &viewport);

            for (key, token) in &st.in_flight {
                if !wanted.contains_key(key) {
                    token.cancel();
                }
            }

            let mut remaining = HashMap::with_capacity(wanted.len());
            for (key, prio) in wanted {
                if st.delivered.contains(&key) {
                    continue;
                }
                if let Some(image) = st.tiles.get(&key) {
                    ready.push((prio, RenderedTile { key, image }));
                    st.delivered.insert(key);
                } else {
                    remaining.insert(key, prio);
                }
            }
            // Rebuild the queue from scratch: everything stale disappears.
            st.queue.clear();
            let mut n = 0;
            for (&key, &prio) in &remaining {
                if !st.in_flight.contains_key(&key) && st.prepared.contains(&key.page) {
                    st.push(key, prio);
                    n += 1;
                }
            }
            st.wanted = remaining;
            jobs = n;
            debug!(
                wanted = st.wanted.len(),
                cached = ready.len(),
                jobs,
                "viewport"
            );
        }
        shared.wake_preparer.notify_one();
        ready.sort_by_key(|(p, _)| *p);
        for (_, tile) in ready {
            (shared.sink)(RenderEvent::Tile(tile));
        }
        spawn_workers(shared, jobs);
    }

    /// Drops every prepared page and tile, e.g. after a layer was toggled.
    /// Work already running finishes but its results are thrown away.
    pub fn invalidate(&self) {
        {
            let mut st = self.shared.state.lock();
            st.epoch += 1;
            st.prepared.remove_where(|_| true);
            st.tiles.remove_where(|_| true);
            st.delivered.clear();
            st.failed.clear();
            st.wanted.clear();
            st.queue.clear();
            for token in st.in_flight.values() {
                token.cancel();
            }
        }
        (self.shared.sink)(RenderEvent::Invalidated);
    }

    pub fn stats(&self) -> Stats {
        let st = self.shared.state.lock();
        Stats {
            wanted: st.wanted.len(),
            queued: st.queue.len(),
            in_flight: st.in_flight.len(),
            preparing: st.preparing.is_some(),
            cached_tiles: st.tiles.len(),
            cached_bytes: st.tiles.cost(),
        }
    }

    /// Whether every wanted tile has been delivered (or failed).
    pub fn is_idle(&self) -> bool {
        let st = self.shared.state.lock();
        st.wanted.is_empty() && st.in_flight.is_empty() && st.preparing.is_none()
    }
}

impl<R: PageRenderer> Drop for Scheduler<R> {
    fn drop(&mut self) {
        let mut st = self.shared.state.lock();
        st.closed = true;
        st.wanted.clear();
        st.queue.clear();
        for token in st.in_flight.values() {
            token.cancel();
        }
        drop(st);
        self.shared.wake_preparer.notify_all();
    }
}

fn wanted_tiles<R: PageRenderer>(renderer: &R, v: &Viewport) -> HashMap<TileKey, Priority> {
    let mut wanted = HashMap::new();
    let regions = v
        .visible
        .iter()
        .map(|r| (0u8, r))
        .chain(v.nearby.iter().map(|r| (1u8, r)));
    for (index, (class, region)) in regions.enumerate() {
        let Some(size) = renderer.page_size(region.page) else {
            continue;
        };
        let level = region.level.clamped();
        let s = level.scale();
        let (cx, cy) = region.rect.center();
        for key in tiles_in_rect(region.page, size, level, region.rect) {
            let r = tile_rect(key, size);
            let tx = (r.x0 + r.x1) as f32 / 2.0 / s;
            let ty = (r.y0 + r.y1) as f32 / 2.0 / s;
            let dist = ((tx - cx).hypot(ty - cy)).min(u32::MAX as f32) as u32;
            let prio = Priority {
                class,
                region: u16::try_from(index).unwrap_or(u16::MAX),
                dist,
            };
            wanted
                .entry(key)
                .and_modify(|p: &mut Priority| *p = (*p).min(prio))
                .or_insert(prio);
        }
    }
    wanted
}

fn spawn_workers<R: PageRenderer>(shared: &Arc<Shared<R>>, n: usize) {
    for _ in 0..n {
        let task = Arc::clone(shared);
        shared.pool.spawn(move || run_one(&task));
    }
}

/// Pops the most urgent runnable job and renders it.
fn run_one<R: PageRenderer>(shared: &Arc<Shared<R>>) {
    let (key, prepared, token, epoch) = {
        let mut st = shared.state.lock();
        loop {
            let Some(job) = st.queue.pop() else { return };
            if st.wanted.get(&job.key) != Some(&job.prio) || st.in_flight.contains_key(&job.key) {
                continue; // stale or duplicate
            }
            let Some(prepared) = st.prepared.get(&job.key.page) else {
                continue; // the preparer re-queues this page's tiles when ready
            };
            let token = CancelToken::new();
            st.in_flight.insert(job.key, token.clone());
            break (job.key, prepared, token, st.epoch);
        }
    };

    let result = shared.renderer.rasterize(&prepared, key, &token);

    let mut requeue = false;
    let event = {
        let mut st = shared.state.lock();
        st.in_flight.remove(&key);
        if st.epoch != epoch {
            return; // rendered from content that has since been invalidated
        }
        match result {
            Ok(image) => {
                let image = Arc::new(image);
                let cost = image.pixels.len();
                st.tiles.put(key, Arc::clone(&image), cost);
                if st.wanted.remove(&key).is_some() {
                    st.delivered.insert(key);
                    Some(RenderEvent::Tile(RenderedTile { key, image }))
                } else {
                    None
                }
            }
            Err(RenderError::Cancelled) => {
                // Cancelled by an older viewport but wanted again by a newer one.
                if let Some(&prio) = st.wanted.get(&key) {
                    st.push(key, prio);
                    requeue = true;
                }
                None
            }
            Err(e) => {
                warn!(?key, error = %e, "tile failed");
                st.wanted.remove(&key);
                None
            }
        }
    };
    if let Some(event) = event {
        (shared.sink)(event);
    }
    if requeue {
        spawn_workers(shared, 1);
    }
}

/// What the preparer thread does next.
enum Step {
    Prepare(u32),
    Measure,
}

fn preparer_loop<R: PageRenderer>(shared: &Arc<Shared<R>>) {
    loop {
        let step = {
            let mut st = shared.state.lock();
            loop {
                if st.closed {
                    return;
                }
                if let Some(page) = st.next_page_to_prepare() {
                    st.preparing = Some(page);
                    break Step::Prepare(page);
                }
                if !st.measured {
                    break Step::Measure;
                }
                shared.wake_preparer.wait(&mut st);
            }
        };
        let page = match step {
            Step::Prepare(page) => page,
            Step::Measure => {
                measure_step(shared);
                continue;
            }
        };

        let epoch = shared.state.lock().epoch;
        let result = {
            let _span = info_span!("render.prepare", page).entered();
            shared.renderer.prepare(page)
        };

        let mut failure = None;
        let jobs = {
            let mut st = shared.state.lock();
            st.preparing = None;
            if st.epoch != epoch {
                // Invalidated while preparing: prepare again if still wanted.
                drop(st);
                shared.wake_preparer.notify_one();
                continue;
            }
            match result {
                Ok(prepared) => {
                    st.prepared.put(page, Arc::new(prepared), 1);
                    let pending: Vec<_> = st
                        .wanted
                        .iter()
                        .filter(|(k, _)| k.page == page && !st.in_flight.contains_key(*k))
                        .map(|(k, p)| (*k, *p))
                        .collect();
                    for (key, prio) in &pending {
                        st.push(*key, *prio);
                    }
                    pending.len()
                }
                Err(e) => {
                    warn!(page, error = %e, "page failed to prepare");
                    st.failed.insert(page);
                    st.wanted.retain(|k, _| k.page != page);
                    failure = Some(RenderEvent::PageFailed {
                        page,
                        message: e.to_string(),
                    });
                    0
                }
            }
        };
        if let Some(event) = failure {
            (shared.sink)(event);
        }
        spawn_workers(shared, jobs);
    }
}

/// Measures one batch of estimated page sizes in idle time. Tiles rendered
/// for a page whose size turned out different are dropped: the frontend
/// relayouts and asks for them again.
fn measure_step<R: PageRenderer>(shared: &Arc<Shared<R>>) {
    let result = {
        let _span = info_span!("render.measure").entered();
        shared.renderer.measure_more()
    };
    let Some(measured) = result else {
        shared.state.lock().measured = true;
        return;
    };
    if !measured.changed.is_empty() {
        let changed: HashSet<u32> = measured.changed.iter().copied().collect();
        let mut st = shared.state.lock();
        st.tiles.remove_where(|k| changed.contains(&k.page));
        st.delivered.retain(|k| !changed.contains(&k.page));
        st.wanted.retain(|k, _| !changed.contains(&k.page));
        for (key, token) in &st.in_flight {
            if changed.contains(&key.page) {
                token.cancel();
            }
        }
    }
    (shared.sink)(RenderEvent::PageSizes(measured));
}
