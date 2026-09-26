// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tile pyramid: discrete zoom levels and fixed-size tiles per level.

use doc::PageSize;
use fitz::IRect;
use serde::{Deserialize, Serialize};

/// Tile edge in device pixels (ADR 0002/0003: fewer, larger tiles win).
pub const TILE_SIZE: u32 = 512;

/// Lowest and highest supported levels: 2^(-12/2) = 1/64 and 2^(18/2) = 512
/// device pixels per point (51 200 % at 96 dpi on a 1x display).
pub const MIN_LEVEL: i8 = -12;
pub const MAX_LEVEL: i8 = 18;

/// Discrete zoom level; level `l` renders at `sqrt(2)^l` device pixels per point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ZoomLevel(pub i8);

impl ZoomLevel {
    pub fn scale(self) -> f32 {
        2f32.powf(f32::from(self.0) / 2.0)
    }

    /// Smallest level whose scale is at least `device_scale`, so tiles are
    /// never upscaled on screen.
    pub fn for_scale(device_scale: f32) -> Self {
        if !device_scale.is_finite() || device_scale <= 0.0 {
            return Self(0);
        }
        // Tolerance so an exact power of sqrt(2) does not round up a level.
        let level = (2.0 * device_scale.log2() - 1e-4).ceil();
        Self(level.clamp(f32::from(MIN_LEVEL), f32::from(MAX_LEVEL)) as i8)
    }

    pub fn clamped(self) -> Self {
        Self(self.0.clamp(MIN_LEVEL, MAX_LEVEL))
    }
}

/// Identifies one tile of one page at one zoom level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TileKey {
    pub page: u32,
    pub level: ZoomLevel,
    pub x: u32,
    pub y: u32,
}

/// Page size in device pixels at `level`.
pub fn page_pixels(size: PageSize, level: ZoomLevel) -> (u32, u32) {
    let s = level.scale();
    let px = |v: f32| (v * s).ceil().clamp(1.0, u32::MAX as f32) as u32;
    (px(size.width), px(size.height))
}

/// Device-pixel rectangle covered by `key`, clipped to the page.
pub fn tile_rect(key: TileKey, size: PageSize) -> IRect {
    let (w, h) = page_pixels(size, key.level);
    let x0 = key.x.saturating_mul(TILE_SIZE).min(w);
    let y0 = key.y.saturating_mul(TILE_SIZE).min(h);
    let x1 = x0.saturating_add(TILE_SIZE).min(w);
    let y1 = y0.saturating_add(TILE_SIZE).min(h);
    IRect::new(to_i32(x0), to_i32(y0), to_i32(x1), to_i32(y1))
}

fn to_i32(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// A rectangle in page points (origin top-left, y down, as MuPDF uses).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PageRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl PageRect {
    pub fn center(&self) -> (f32, f32) {
        ((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }
}

/// Tiles of `page` at `level` that intersect `rect` (in points), row-major.
pub fn tiles_in_rect(
    page: u32,
    size: PageSize,
    level: ZoomLevel,
    rect: PageRect,
) -> impl Iterator<Item = TileKey> {
    let s = level.scale();
    let (w, h) = page_pixels(size, level);
    let cols = w.div_ceil(TILE_SIZE);
    let rows = h.div_ceil(TILE_SIZE);
    let to_tile = |v: f32, max: u32| -> u32 {
        let px = (v * s).max(0.0);
        ((px / TILE_SIZE as f32).floor() as u32).min(max.saturating_sub(1))
    };
    let valid = rect.x1 > rect.x0 && rect.y1 > rect.y0 && rect.x1 > 0.0 && rect.y1 > 0.0;
    let (x0, x1) = (to_tile(rect.x0, cols), to_tile(rect.x1, cols));
    let (y0, y1) = (to_tile(rect.y0, rows), to_tile(rect.y1, rows));
    // An invalid rect yields no rows.
    let ys = (y0..=y1).take(if valid { usize::MAX } else { 0 });
    ys.flat_map(move |y| (x0..=x1).map(move |x| TileKey { page, level, x, y }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTER: PageSize = PageSize {
        width: 612.0,
        height: 792.0,
    };

    #[test]
    fn levels_are_powers_of_sqrt2() {
        assert_eq!(ZoomLevel(0).scale(), 1.0);
        assert_eq!(ZoomLevel(2).scale(), 2.0);
        assert!((ZoomLevel(1).scale() - std::f32::consts::SQRT_2).abs() < 1e-6);
        assert_eq!(ZoomLevel::for_scale(1.0), ZoomLevel(0));
        assert_eq!(ZoomLevel::for_scale(1.01), ZoomLevel(1));
        assert_eq!(ZoomLevel::for_scale(2.0), ZoomLevel(2));
        assert_eq!(ZoomLevel::for_scale(0.3), ZoomLevel(-3));
        assert_eq!(ZoomLevel::for_scale(1e9), ZoomLevel(MAX_LEVEL));
        assert_eq!(ZoomLevel::for_scale(f32::NAN), ZoomLevel(0));
    }

    #[test]
    fn edge_tiles_are_clipped_to_the_page() {
        let key = TileKey {
            page: 0,
            level: ZoomLevel(0),
            x: 1,
            y: 1,
        };
        assert_eq!(tile_rect(key, LETTER), IRect::new(512, 512, 612, 792));
        let outside = TileKey { x: 9, ..key };
        assert!(tile_rect(outside, LETTER).is_empty());
    }

    #[test]
    fn tiles_cover_the_requested_rect() {
        let all: Vec<_> = tiles_in_rect(
            3,
            LETTER,
            ZoomLevel(2),
            PageRect {
                x0: 0.0,
                y0: 0.0,
                x1: 612.0,
                y1: 792.0,
            },
        )
        .collect();
        // 1224 x 1584 px -> 3 x 4 tiles.
        assert_eq!(all.len(), 12);
        assert!(all.iter().all(|k| k.page == 3 && k.level == ZoomLevel(2)));

        let one: Vec<_> = tiles_in_rect(
            0,
            LETTER,
            ZoomLevel(2),
            PageRect {
                x0: 300.0,
                y0: 300.0,
                x1: 310.0,
                y1: 310.0,
            },
        )
        .collect();
        assert_eq!(
            one,
            vec![TileKey {
                page: 0,
                level: ZoomLevel(2),
                x: 1,
                y: 1
            }]
        );

        let empty = PageRect {
            x0: 10.0,
            y0: 10.0,
            x1: 10.0,
            y1: 20.0,
        };
        assert_eq!(tiles_in_rect(0, LETTER, ZoomLevel(0), empty).count(), 0);
    }
}
