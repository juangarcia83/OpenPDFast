// SPDX-License-Identifier: AGPL-3.0-or-later

// Integration tests: panicking on unexpected errors is the point.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use render::{
    MupdfRenderer, PageRect, PageRegion, PageRenderer, RenderEvent, Scheduler, SchedulerConfig,
    Viewport, ZoomLevel, tiles_in_rect,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/corpus/fixtures")
        .join(name)
}

fn scheduler(name: &str) -> (Scheduler<MupdfRenderer>, mpsc::Receiver<RenderEvent>) {
    scheduler_with(name, SchedulerConfig::default())
}

fn scheduler_with(
    name: &str,
    config: SchedulerConfig,
) -> (Scheduler<MupdfRenderer>, mpsc::Receiver<RenderEvent>) {
    let doc = doc::Document::open(&fixture(name), None).unwrap();
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    let s = Scheduler::new(MupdfRenderer::new(doc), config, sink).unwrap();
    (s, rx)
}

fn whole(page: u32, w: f32, h: f32, level: ZoomLevel) -> PageRegion {
    PageRegion {
        page,
        rect: PageRect {
            x0: 0.0,
            y0: 0.0,
            x1: w,
            y1: h,
        },
        level,
    }
}

fn wait_idle(s: &Scheduler<MupdfRenderer>, timeout: Duration) -> Duration {
    let start = Instant::now();
    while !s.is_idle() {
        assert!(
            start.elapsed() < timeout,
            "not idle after {timeout:?}: {:?}",
            s.stats()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    start.elapsed()
}

#[test]
fn delivers_every_visible_tile_once() {
    let (s, rx) = scheduler("paper-3.pdf");
    let level = ZoomLevel(2);
    s.set_viewport(Viewport {
        visible: vec![whole(0, 612.0, 792.0, level)],
        ..Default::default()
    });
    wait_idle(&s, Duration::from_secs(10));
    let tiles: Vec<_> = rx
        .try_iter()
        .filter_map(|e| match e {
            RenderEvent::Tile(t) => Some(t),
            RenderEvent::PageFailed { .. }
            | RenderEvent::PageSizes(_)
            | RenderEvent::Invalidated => None,
        })
        .collect();
    assert_eq!(tiles.len(), 12, "1224x1584 px = 3x4 tiles");
    for t in &tiles {
        assert_eq!(
            t.image.pixels.len(),
            (t.image.width * t.image.height * 4) as usize
        );
    }

    // Same viewport again: everything is already delivered, nothing is sent.
    s.set_viewport(Viewport {
        visible: vec![whole(0, 612.0, 792.0, level)],
        ..Default::default()
    });
    wait_idle(&s, Duration::from_secs(1));
    assert_eq!(rx.try_iter().count(), 0);

    // After the frontend evicts a tile it is re-sent from the cache.
    let evicted = tiles[0].key;
    s.set_viewport(Viewport {
        visible: vec![whole(0, 612.0, 792.0, level)],
        evicted: vec![evicted],
        ..Default::default()
    });
    wait_idle(&s, Duration::from_secs(1));
    let again: Vec<_> = rx.try_iter().collect();
    assert!(matches!(again.as_slice(), [RenderEvent::Tile(t)] if t.key == evicted));
}

#[test]
fn visible_tiles_come_before_nearby_ones() {
    // One rasterizer thread makes completion order equal to start order;
    // with more threads a quick nearby tile can legitimately finish first.
    let config = SchedulerConfig {
        threads: 1,
        ..SchedulerConfig::default()
    };
    let (s, rx) = scheduler_with("plan-a4.pdf", config);
    let level = ZoomLevel(4); // 2380 x 3368 px = 5 x 7 tiles
    let visible = PageRegion {
        page: 0,
        rect: PageRect {
            x0: 0.0,
            y0: 0.0,
            x1: 200.0,
            y1: 150.0,
        },
        level,
    };
    s.set_viewport(Viewport {
        visible: vec![visible],
        nearby: vec![whole(0, 595.0, 842.0, level)],
        ..Default::default()
    });
    wait_idle(&s, Duration::from_secs(30));
    let size = doc::PageSize {
        width: 595.0,
        height: 842.0,
    };
    let visible_keys: Vec<_> = tiles_in_rect(0, size, level, visible.rect).collect();
    let order: Vec<_> = rx
        .try_iter()
        .filter_map(|e| {
            if let RenderEvent::Tile(t) = e {
                Some(t.key)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(order.len(), 35);
    let first: Vec<_> = order[..visible_keys.len()].to_vec();
    for k in &visible_keys {
        assert!(
            first.contains(k),
            "visible tile {k:?} delivered late: {order:?}"
        );
    }
}

#[test]
fn rapid_zoom_changes_leave_no_zombie_work() {
    let (s, rx) = scheduler("plan-a4.pdf");
    // 20 fast zoom changes, like a user spinning the wheel.
    for i in 0..20 {
        s.set_viewport(Viewport {
            visible: vec![whole(0, 595.0, 842.0, ZoomLevel(i % 8))],
            ..Default::default()
        });
    }
    let final_level = ZoomLevel(19 % 8);
    let elapsed = wait_idle(&s, Duration::from_secs(30));
    let stats = s.stats();
    assert_eq!((stats.wanted, stats.in_flight), (0, 0));
    // Tiles of the final level must all have arrived.
    let size = doc::PageSize {
        width: 595.0,
        height: 842.0,
    };
    let expected = tiles_in_rect(
        0,
        size,
        final_level,
        PageRect {
            x0: 0.0,
            y0: 0.0,
            x1: 595.0,
            y1: 842.0,
        },
    )
    .count();
    let got = rx
        .try_iter()
        .filter(|e| matches!(e, RenderEvent::Tile(t) if t.key.level == final_level))
        .count();
    assert_eq!(got, expected);
    eprintln!("drained in {elapsed:?}");
}

#[test]
fn unknown_pages_and_empty_viewports_are_ignored() {
    let (s, rx) = scheduler("paper-3.pdf");
    s.set_viewport(Viewport {
        visible: vec![whole(99, 612.0, 792.0, ZoomLevel(0))],
        ..Default::default()
    });
    s.set_viewport(Viewport::default());
    wait_idle(&s, Duration::from_secs(1));
    assert_eq!(rx.try_iter().count(), 0);
}

#[test]
fn estimated_page_sizes_are_measured_in_idle_time() {
    let (s, rx) = scheduler("mixed-sizes-100.pdf");
    // Pages 60-69 are Letter; page 70 starts as an estimate copied from
    // page 63 (the last one measured when opening) but is A4 landscape.
    let start = Instant::now();
    let mut measured = Vec::new();
    let mut changed = Vec::new();
    while measured.len() < 36 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "sizes not measured"
        );
        if let Ok(RenderEvent::PageSizes(m)) = rx.recv_timeout(Duration::from_millis(50)) {
            changed.extend_from_slice(&m.changed);
            measured.extend(
                m.sizes
                    .iter()
                    .enumerate()
                    .map(|(i, sz)| (m.first + i as u32, *sz)),
            );
        }
    }
    let page = |n: u32| measured.iter().find(|(p, _)| *p == n).unwrap().1;
    assert_eq!(measured.first().map(|(p, _)| *p), Some(64));
    assert_eq!((page(70).width, page(70).height), (842.0, 595.0));
    assert_eq!((page(85).width, page(85).height), (612.0, 792.0));
    assert!(
        changed.contains(&70) && !changed.contains(&65),
        "{changed:?}"
    );
    assert_eq!(s.renderer().page_size(70).unwrap().width, 842.0);
}

#[test]
fn toggling_a_layer_invalidates_and_rerenders() {
    let (s, rx) = scheduler("plan-a4.pdf");
    let level = ZoomLevel(0);
    let view = || Viewport {
        visible: vec![whole(0, 595.0, 842.0, level)],
        ..Default::default()
    };
    s.set_viewport(view());
    wait_idle(&s, Duration::from_secs(10));
    let first: Vec<_> = rx
        .try_iter()
        .filter_map(|e| {
            if let RenderEvent::Tile(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .collect();
    assert!(!first.is_empty());

    let layers = s.renderer().set_layer(1, false).unwrap();
    assert!(!layers[1].visible);
    s.invalidate();
    assert!(rx.try_iter().any(|e| matches!(e, RenderEvent::Invalidated)));

    // The frontend dropped everything and sends its viewport again.
    s.set_viewport(view());
    wait_idle(&s, Duration::from_secs(10));
    let second: Vec<_> = rx
        .try_iter()
        .filter_map(|e| {
            if let RenderEvent::Tile(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(second.len(), first.len());
    let changed = second
        .iter()
        .filter(|t| {
            first
                .iter()
                .any(|f| f.key == t.key && f.image.pixels != t.image.pixels)
        })
        .count();
    assert!(
        changed > 0,
        "tiles must be re-rendered without the hidden layer"
    );
}
