// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `PageRenderer` trait and the MuPDF backend.

use std::sync::Arc;

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use doc::{DisplayList, Document, PageSize};
use fitz::{Cookie, IRect, Matrix, Rect};
use parking_lot::{Mutex, RwLock};
use tracing::{debug, info_span};

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

/// Tuning of the MuPDF backend.
#[derive(Debug, Clone)]
pub struct MupdfOptions {
    /// Pages whose display list takes at least this long to build are
    /// "heavy": their tiles are drawn from per-cell sub-lists (ADR 0002).
    pub heavy_page: Duration,
    /// Edge of a cell, in points.
    pub cell_pt: f32,
    /// Tiles spanning more cells than this use the whole list instead
    /// (zoomed out, a tile covers most of the page anyway).
    pub max_cells_per_tile: usize,
}

impl Default for MupdfOptions {
    fn default() -> Self {
        Self {
            heavy_page: Duration::from_millis(40),
            cell_pt: 256.0,
            max_cells_per_tile: 4,
        }
    }
}

/// Extra points recorded around each cell so objects that reach a cell
/// through pixel rounding at any zoom level are included.
const CELL_MARGIN_PT: f32 = 4.0;

/// A prepared page: the whole display list and, for heavy pages, a grid of
/// sub-lists extracted lazily the first time a tile needs them.
///
/// Every node of a display list is visited on each run even when culled, so
/// on a plan with a million paths each tile costs the whole page. A cell
/// sub-list only holds what intersects the cell, and each cell only paints
/// its own pixels of the tile, so the result is identical to the whole list.
pub struct PageContent {
    list: DisplayList,
    grid: Option<CellGrid>,
}

struct CellGrid {
    cell_pt: f32,
    cols: usize,
    rows: usize,
    /// `None` inside a cell means extraction failed: use the whole list.
    cells: Vec<OnceLock<Option<DisplayList>>>,
}

impl PageContent {
    pub fn is_partitioned(&self) -> bool {
        self.grid.is_some()
    }

    /// How many cell sub-lists have been extracted so far.
    pub fn extracted_cells(&self) -> usize {
        self.grid
            .as_ref()
            .map_or(0, |g| g.cells.iter().filter(|c| c.get().is_some()).count())
    }
}

/// MuPDF backend: each page becomes a display list, tiles are rasterized
/// from it in parallel (ADR 0002).
pub struct MupdfRenderer {
    // MuPDF documents are not thread-safe; only the scheduler's single
    // preparer thread takes this lock, so it is never contended.
    doc: Mutex<Document>,
    sizes: RwLock<Vec<PageSize>>,
    options: MupdfOptions,
}

/// Pages measured per idle step: small enough to keep prepare latency low.
const MEASURE_CHUNK: usize = 128;

impl MupdfRenderer {
    pub fn new(doc: Document) -> Self {
        Self::with_options(doc, MupdfOptions::default())
    }

    pub fn with_options(doc: Document, options: MupdfOptions) -> Self {
        let sizes = RwLock::new(doc.info().pages.clone());
        Self {
            doc: Mutex::new(doc),
            sizes,
            options,
        }
    }

    pub fn layers(&self) -> Result<Vec<doc::LayerInfo>, RenderError> {
        Ok(self.doc.lock().layers()?)
    }

    /// Toggles a layer. Waits for an in-progress `prepare` to finish (it holds
    /// the document); the caller must then invalidate the scheduler.
    pub fn set_layer(&self, index: u32, visible: bool) -> Result<Vec<doc::LayerInfo>, RenderError> {
        let mut doc = self.doc.lock();
        doc.set_layer(index, visible)?;
        Ok(doc.layers()?)
    }

    /// Draws a tile cell by cell; `None` when it spans too many cells.
    fn render_cells(
        &self,
        list: &DisplayList,
        grid: &CellGrid,
        rect: IRect,
        s: f32,
        cancel: &CancelToken,
    ) -> Result<Option<fitz::Pixmap>, RenderError> {
        let cell_of = |px: i32, max: usize| -> usize {
            let pt = px as f32 / s;
            ((pt / grid.cell_pt).floor().max(0.0) as usize).min(max - 1)
        };
        let (cx0, cx1) = (cell_of(rect.x0, grid.cols), cell_of(rect.x1 - 1, grid.cols));
        let (cy0, cy1) = (cell_of(rect.y0, grid.rows), cell_of(rect.y1 - 1, grid.rows));
        if (cx1 - cx0 + 1) * (cy1 - cy0 + 1) > self.options.max_cells_per_tile {
            return Ok(None);
        }
        let ctm = Matrix::scale(s, s);
        let pix = fitz::Pixmap::new_white_rgba(rect)?;
        // Cell edges in device pixels. Neighbours round the same value, so
        // their clips never overlap or leave a gap; the first and last cells
        // extend to the tile edges.
        let edge = |i: usize| (i as f32 * grid.cell_pt * s).round() as i32;
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                let clip = IRect::new(
                    if cx == cx0 { rect.x0 } else { edge(cx) },
                    if cy == cy0 { rect.y0 } else { edge(cy) },
                    if cx == cx1 { rect.x1 } else { edge(cx + 1) },
                    if cy == cy1 { rect.y1 } else { edge(cy + 1) },
                );
                if clip.is_empty() {
                    continue;
                }
                let sub = grid.cells[cy * grid.cols + cx].get_or_init(|| {
                    let _span = info_span!("render.extract_cell", cx, cy).entered();
                    let (x, y) = (cx as f32 * grid.cell_pt, cy as f32 * grid.cell_pt);
                    let area = Rect::new(
                        x - CELL_MARGIN_PT,
                        y - CELL_MARGIN_PT,
                        x + grid.cell_pt + CELL_MARGIN_PT,
                        y + grid.cell_pt + CELL_MARGIN_PT,
                    );
                    list.extract(area).ok()
                });
                let source = sub.as_ref().unwrap_or(list);
                source.render_into(&pix, ctm, clip, Some(cancel.cookie()))?;
                if cancel.is_cancelled() {
                    return Err(RenderError::Cancelled);
                }
            }
        }
        Ok(Some(pix))
    }
}

impl PageRenderer for MupdfRenderer {
    type Prepared = PageContent;

    fn page_count(&self) -> u32 {
        u32::try_from(self.sizes.read().len()).unwrap_or(u32::MAX)
    }

    fn page_size(&self, page: u32) -> Option<PageSize> {
        self.sizes.read().get(page as usize).copied()
    }

    fn prepare(&self, page: u32) -> Result<PageContent, RenderError> {
        if page >= self.page_count() {
            return Err(RenderError::NoSuchPage(page));
        }
        let started = Instant::now();
        let list = self.doc.lock().display_list(page as usize)?;
        let grid = (started.elapsed() >= self.options.heavy_page).then(|| {
            let b = list.bounds();
            let cell_pt = self.options.cell_pt.max(16.0);
            let cols = ((b.x1.max(1.0) / cell_pt).ceil() as usize).max(1);
            let rows = ((b.y1.max(1.0) / cell_pt).ceil() as usize).max(1);
            debug!(page, cols, rows, "heavy page: tiles are drawn from cells");
            CellGrid {
                cell_pt,
                cols,
                rows,
                cells: (0..cols * rows).map(|_| OnceLock::new()).collect(),
            }
        });
        Ok(PageContent { list, grid })
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
        content: &PageContent,
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
        let from_cells = match &content.grid {
            Some(grid) => self.render_cells(&content.list, grid, rect, s, cancel)?,
            None => None,
        };
        let pix = match from_cells {
            Some(pix) => pix,
            None => content
                .list
                .render(Matrix::scale(s, s), rect, Some(cancel.cookie()))?,
        };
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
