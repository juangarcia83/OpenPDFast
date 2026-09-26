// SPDX-License-Identifier: AGPL-3.0-or-later

// Integration tests: panicking on unexpected errors is the point.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::time::Duration;

use render::{
    CancelToken, MupdfOptions, MupdfRenderer, PageRenderer, TILE_SIZE, TileKey, ZoomLevel,
    page_pixels,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/corpus/fixtures")
        .join(name)
}

fn renderer(name: &str, options: MupdfOptions) -> MupdfRenderer {
    let doc = doc::Document::open(&fixture(name), None).unwrap();
    MupdfRenderer::with_options(doc, options)
}

/// Number of differing pixels and the largest channel difference.
fn compare(a: &[u8], b: &[u8]) -> (usize, u8) {
    let diff = a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count();
    let max = a
        .iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0);
    (diff, max)
}

/// Tiles drawn cell by cell must match tiles drawn from the whole display
/// list, including tiles that straddle cell edges at zoom levels whose scale
/// is not a power of two. Only anti-aliasing may move a few levels, because
/// each cell is drawn with its own clip (see the fitz test on clipped runs).
#[test]
fn cell_tiles_match_whole_list_tiles() {
    for file in ["plan-a4.pdf", "clips-groups.pdf"] {
        let cells = renderer(
            file,
            MupdfOptions {
                heavy_page: Duration::ZERO,
                cell_pt: 100.0,
                max_cells_per_tile: 64,
            },
        );
        let whole = renderer(
            file,
            MupdfOptions {
                heavy_page: Duration::MAX,
                ..MupdfOptions::default()
            },
        );
        let a = cells.prepare(0).unwrap();
        let b = whole.prepare(0).unwrap();
        assert!(a.is_partitioned() && !b.is_partitioned());
        let size = cells.page_size(0).unwrap();
        let cancel = CancelToken::new();
        for level in [0, 3] {
            let level = ZoomLevel(level);
            let (w, h) = page_pixels(size, level);
            for y in 0..h.div_ceil(TILE_SIZE) {
                for x in 0..w.div_ceil(TILE_SIZE) {
                    let key = TileKey {
                        page: 0,
                        level,
                        x,
                        y,
                    };
                    let from_cells = cells.rasterize(&a, key, &cancel).unwrap();
                    let from_whole = whole.rasterize(&b, key, &cancel).unwrap();
                    let (diff, max) = compare(&from_cells.pixels, &from_whole.pixels);
                    let total = from_cells.pixels.len() / 4;
                    assert!(
                        max <= 16 && diff * 100 < total * 3,
                        "{file}: tile {key:?}: {diff}/{total} px differ, max delta {max}"
                    );
                }
            }
        }
        assert!(a.extracted_cells() > 0);
    }
}

#[test]
fn light_pages_are_not_partitioned() {
    let r = renderer("paper-3.pdf", MupdfOptions::default());
    assert!(!r.prepare(0).unwrap().is_partitioned());
}
