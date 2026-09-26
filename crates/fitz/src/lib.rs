// SPDX-License-Identifier: AGPL-3.0-or-later

//! Small safe layer over `mupdf-sys` (ADR 0004).
//!
//! This is the only crate that talks to MuPDF directly. Rules:
//! - every call that can throw goes through a `mupdf_*` wrapper, which runs it
//!   inside `fz_try` and reports errors through an out pointer;
//! - every thread uses its own `fz_context`, cloned from one process-wide base
//!   context that owns the locks, the resource store and the glyph cache;
//! - [`Document`] and [`Page`] may move between threads but are not shared;
//!   [`DisplayList`] is immutable once recorded and can be rasterized from
//!   many threads at once.

mod context;
mod cookie;
mod display_list;
mod document;
mod error;
mod geometry;
mod layers;
mod pixmap;

pub use cookie::Cookie;
pub use display_list::DisplayList;
pub use document::{Document, MetadataKey, Page};
pub use error::{Error, ErrorKind, Result};
pub use geometry::{IRect, Matrix, Rect};
pub use layers::{Layer, LayerKind};
pub use pixmap::Pixmap;
