// SPDX-License-Identifier: AGPL-3.0-or-later

//! Render benchmarks over the synthetic corpus (AGENTS.md §4).
//!
//! Get the corpus first: `pnpm corpus` (generates and downloads it).
//! Run: `cargo bench -p render`.

use std::path::PathBuf;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use doc::{Document, PageSize};
use fitz::Matrix;
use rayon::prelude::*;
use render::{TILE_SIZE, ZoomLevel, page_pixels, tile_rect};

/// Synthetic files live in `generated/`, real-world ones in `real/`.
const DOCS: [&str; 6] = [
    "paper-60.pdf",
    "big-1000p.pdf",
    "plan-a0-layers.pdf",
    "pgfmanual-3.1.12.pdf",
    "ID_Carey_20200331_TM_geo.pdf",
    "originofspecies00darwuoft.pdf",
];

fn corpus(name: &str) -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bench/corpus");
    let found = ["generated", "real"]
        .iter()
        .map(|d| root.join(d).join(name))
        .find(|p| p.exists());
    if found.is_none() {
        eprintln!("skipping {name}: run `pnpm corpus` first");
    }
    found
}

/// Level at which the page is ~1200 device pixels wide (a fit-width window).
fn fit_width_level(size: PageSize) -> ZoomLevel {
    ZoomLevel::for_scale(1200.0 / size.width)
}

fn all_tiles(size: PageSize, level: ZoomLevel) -> Vec<render::TileKey> {
    let (w, h) = page_pixels(size, level);
    let (cols, rows) = (w.div_ceil(TILE_SIZE), h.div_ceil(TILE_SIZE));
    (0..rows)
        .flat_map(|y| {
            (0..cols).map(move |x| render::TileKey {
                page: 0,
                level,
                x,
                y,
            })
        })
        .collect()
}

fn render_page(list: &doc::DisplayList, size: PageSize, level: ZoomLevel, parallel: bool) {
    let s = level.scale();
    let render = |key| {
        let pix = list.render(Matrix::scale(s, s), tile_rect(key, size), None);
        std::hint::black_box(pix.map(|p| p.to_rgba()).ok());
    };
    let tiles = all_tiles(size, level);
    if parallel {
        tiles.into_par_iter().for_each(render);
    } else {
        tiles.into_iter().for_each(render);
    }
}

fn open(c: &mut Criterion) {
    let mut g = c.benchmark_group("open");
    for name in DOCS {
        let Some(path) = corpus(name) else { continue };
        g.bench_with_input(BenchmarkId::from_parameter(name), &path, |b, path| {
            b.iter(|| Document::open(path, None).map(|d| d.page_count()).ok());
        });
    }
    g.finish();
}

/// Open + interpret page 1 + rasterize all its tiles at fit-width: the time
/// until the first page is fully sharp (budget: < 150 ms, AGENTS.md §4).
fn first_page(c: &mut Criterion) {
    let mut g = c.benchmark_group("first_page");
    g.sample_size(10).measurement_time(Duration::from_secs(10));
    for name in DOCS {
        let Some(path) = corpus(name) else { continue };
        g.bench_with_input(BenchmarkId::from_parameter(name), &path, |b, path| {
            b.iter(|| {
                let Ok(doc) = Document::open(path, None) else {
                    return;
                };
                let size = doc.info().pages[0];
                if let Ok(list) = doc.display_list(0) {
                    render_page(&list, size, fit_width_level(size), true);
                }
            });
        });
    }
    g.finish();
}

/// Tiles per second from a prepared display list at 2x (level 2).
fn tiles(c: &mut Criterion) {
    let mut g = c.benchmark_group("tiles");
    g.sample_size(10).measurement_time(Duration::from_secs(10));
    for name in DOCS {
        let Some(path) = corpus(name) else { continue };
        let Ok(doc) = Document::open(&path, None) else {
            continue;
        };
        let size = doc.info().pages[0];
        let Ok(list) = doc.display_list(0) else {
            continue;
        };
        let level = ZoomLevel(2);
        g.throughput(Throughput::Elements(all_tiles(size, level).len() as u64));
        for parallel in [false, true] {
            let id = BenchmarkId::new(if parallel { "parallel" } else { "serial" }, name);
            g.bench_function(id, |b| b.iter(|| render_page(&list, size, level, parallel)));
        }
    }
    g.finish();
}

criterion_group!(benches, open, first_page, tiles);
criterion_main!(benches);
