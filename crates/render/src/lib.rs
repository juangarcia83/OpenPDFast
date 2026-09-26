// SPDX-License-Identifier: AGPL-3.0-or-later

//! Page rendering: the `PageRenderer` trait, backends, tiling, tile cache and scheduler.
//!
//! This crate must not depend on Tauri so it can be used from CLIs and tests.

mod cache;
mod renderer;
mod scheduler;
mod tile;

pub use renderer::{
    CancelToken, MeasuredSizes, MupdfRenderer, PageRenderer, RenderError, RgbaImage,
};
pub use scheduler::{
    PageRegion, RenderEvent, RenderedTile, Scheduler, SchedulerConfig, Sink, Stats, Viewport,
};
pub use tile::{
    MAX_LEVEL, MIN_LEVEL, PageRect, TILE_SIZE, TileKey, ZoomLevel, page_pixels, tile_rect,
    tiles_in_rect,
};
