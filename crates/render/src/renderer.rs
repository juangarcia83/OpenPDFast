// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `PageRenderer` trait and the MuPDF backend.

use std::sync::Arc;

use doc::{DisplayList, Document, PageSize};
use fitz::{Cookie, IRect, Matrix};
use parking_lot::{Mutex, RwLock};
use tracing::info_span;

use crate::tile::{TileKey, tile_rect};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("page {0} does not exist")]
    NoSuchPage(u32),
    #[error("rendering was cancelled")]
    Cancelled,
    #[error("backend error: {0}")]
    Backend(String),
}

impl From<fitz::Error> for RenderError {
    fn from(e: fitz::Error) -> Self {
        match e.kind() {
            fitz::ErrorKind::Aborted => Self::Cancelled,
            _ => Self::Backend(e.to_string()),
        }
    }
}

/// Cooperative cancellation shared between the scheduler and a render.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<Cookie>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.abort();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_aborted()
    }

    /// The MuPDF cookie that makes an in-flight render stop early.
    pub fn cookie(&self) -> &Cookie {
        &self.0
    }
}

/// A rendering backend. `prepare` does the expensive, per-page interpretation
/// once; `rasterize` turns the prepared page into tiles and must be callable
/// from many threads at once.
pub trait PageRenderer: Send + Sync + 'static {
    type Prepared: Send + Sync + 'static;

    fn page_count(&self) -> u32;

    fn page_size(&self, page: u32) -> Option<PageSize>;

    /// Called by the scheduler from a single thread only.
    fn prepare(&self, page: u32) -> Result<Self::Prepared, RenderError>;

    /// Background work for idle time, called from the same thread as
    /// [`prepare`](Self::prepare): measures the next batch of pages whose size
    /// was only estimated. Returns `None` when there is nothing left to do.
    fn measure_more(&self) -> Option<MeasuredSizes> {
        None
    }

    /// Returns tightly packed RGBA rows for the tile, plus its width and height.
    fn rasterize(
        &self,
        prepared: &Self::Prepared,
        key: TileKey,
        cancel: &CancelToken,
    ) -> Result<RgbaImage, RenderError>;
}

/// Exact sizes of pages `first..first + sizes.len()`, replacing estimates.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredSizes {
    pub first: u32,
    pub sizes: Vec<PageSize>,
    /// Pages in this run whose size differs from the estimate.
    pub changed: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// MuPDF backend: each page becomes a display list, tiles are rasterized
/// from it in parallel (ADR 0002).
pub struct MupdfRenderer {
    // MuPDF documents are not thread-safe; only the scheduler's single
    // preparer thread takes this lock, so it is never contended.
    doc: Mutex<Document>,
    sizes: RwLock<Vec<PageSize>>,
}

/// Pages measured per idle step: small enough to keep prepare latency low.
const MEASURE_CHUNK: usize = 128;

impl MupdfRenderer {
    pub fn new(doc: Document) -> Self {
        let sizes = RwLock::new(doc.info().pages.clone());
        Self {
            doc: Mutex::new(doc),
            sizes,
        }
    }
}

impl PageRenderer for MupdfRenderer {
    type Prepared = DisplayList;

    fn page_count(&self) -> u32 {
        u32::try_from(self.sizes.read().len()).unwrap_or(u32::MAX)
    }

    fn page_size(&self, page: u32) -> Option<PageSize> {
        self.sizes.read().get(page as usize).copied()
    }

    fn prepare(&self, page: u32) -> Result<DisplayList, RenderError> {
        if page >= self.page_count() {
            return Err(RenderError::NoSuchPage(page));
        }
        Ok(self.doc.lock().display_list(page as usize)?)
    }

    fn measure_more(&self) -> Option<MeasuredSizes> {
        let measured = self.doc.lock().measure_next(MEASURE_CHUNK)?;
        let mut sizes = self.sizes.write();
        let mut changed = Vec::new();
        for (i, size) in measured.sizes.iter().enumerate() {
            let page = measured.first + i;
            if let Some(slot) = sizes.get_mut(page) {
                if *slot != *size {
                    changed.push(u32::try_from(page).unwrap_or(u32::MAX));
                }
                *slot = *size;
            }
        }
        Some(MeasuredSizes {
            first: u32::try_from(measured.first).unwrap_or(u32::MAX),
            sizes: measured.sizes,
            changed,
        })
    }

    fn rasterize(
        &self,
        list: &DisplayList,
        key: TileKey,
        cancel: &CancelToken,
    ) -> Result<RgbaImage, RenderError> {
        let size = self
            .page_size(key.page)
            .ok_or(RenderError::NoSuchPage(key.page))?;
        let rect: IRect = tile_rect(key, size);
        if rect.is_empty() {
            return Err(RenderError::Backend("tile outside the page".into()));
        }
        let _span = info_span!(
            "render.raster",
            page = key.page,
            level = key.level.0,
            x = key.x,
            y = key.y
        )
        .entered();
        let s = key.level.scale();
        let pix = list.render(Matrix::scale(s, s), rect, Some(cancel.cookie()))?;
        if cancel.is_cancelled() {
            return Err(RenderError::Cancelled);
        }
        Ok(RgbaImage {
            width: pix.width(),
            height: pix.height(),
            pixels: pix.to_rgba()?,
        })
    }
}
