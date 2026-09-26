// SPDX-License-Identifier: AGPL-3.0-or-later

//! Document abstraction: open, metadata, outline, text, links and optional content (OCG).
//!
//! This crate must not depend on Tauri so it can be used from CLIs and tests.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tracing::{info_span, warn};

pub use fitz::{DisplayList, Page};

/// Pages measured while opening. The rest start with an estimated size and
/// are measured later with [`Document::measure_next`], so opening stays fast
/// on documents with thousands of pages.
pub const EAGER_PAGES: usize = 64;

/// US Letter, used for pages whose box is missing or invalid.
const FALLBACK_PAGE: PageSize = PageSize {
    width: 612.0,
    height: 792.0,
};

/// Size of a page in PDF points (1/72 inch), rotation already applied.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    /// For example `PDF 1.7`.
    pub format: Option<String>,
}

/// Everything the UI needs to lay out a document before rendering anything.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    pub pages: Vec<PageSize>,
    pub metadata: Metadata,
    /// Pages whose box could not be read; shown with a fallback size.
    pub broken_pages: Vec<u32>,
    /// Pages `0..measured_pages` have exact sizes; later ones are estimates
    /// (the last measured size) until measured.
    pub measured_pages: u32,
}

/// How a layer entry behaves in the layers panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LayerKind {
    /// A heading that cannot be toggled.
    Label,
    Checkbox,
    /// Part of a group where only one entry can be visible.
    Radio,
}

/// One entry of the document's optional content (OCG) configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LayerInfo {
    pub index: u32,
    pub name: String,
    pub depth: u32,
    pub kind: LayerKind,
    pub visible: bool,
    pub locked: bool,
}

/// Exact sizes for a run of pages that were previously estimated.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredPages {
    pub first: usize,
    pub sizes: Vec<PageSize>,
    pub broken: Vec<u32>,
}

/// Why a document could not be opened. Messages never contain the password.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("file not found: {0}")]
    NotFound(PathBuf),
    #[error("could not read file: {0}")]
    Io(#[from] std::io::Error),
    #[error("the document is password protected")]
    PasswordRequired,
    #[error("wrong password")]
    WrongPassword,
    #[error("the document has no pages")]
    Empty,
    #[error("unsupported or corrupt document: {0}")]
    Invalid(String),
}

impl From<fitz::Error> for OpenError {
    fn from(e: fitz::Error) -> Self {
        Self::Invalid(e.to_string())
    }
}

/// An open document. Not `Sync`: use it from one thread at a time.
pub struct Document {
    inner: fitz::Document,
    info: DocumentInfo,
}

impl DocumentInfo {
    pub fn all_measured(&self) -> bool {
        self.measured_pages as usize >= self.pages.len()
    }
}

impl Document {
    /// Opens a document, reading only its structure and page boxes; page
    /// contents are interpreted later, on demand.
    pub fn open(path: &Path, password: Option<&str>) -> Result<Self, OpenError> {
        let _span = info_span!("doc.open", path = %path.display()).entered();
        let meta = std::fs::metadata(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => OpenError::NotFound(path.to_owned()),
            _ => OpenError::Io(e),
        })?;
        if !meta.is_file() {
            return Err(OpenError::NotFound(path.to_owned()));
        }

        let mut inner = fitz::Document::open(path)?;
        if inner.needs_password()? {
            match password {
                None => return Err(OpenError::PasswordRequired),
                Some(p) if !inner.authenticate(p)? => return Err(OpenError::WrongPassword),
                Some(_) => {}
            }
        }

        let count = inner.page_count()?;
        if count == 0 {
            return Err(OpenError::Empty);
        }
        // Look pages up lazily instead of loading the whole page tree.
        inner.set_page_tree_cache(false)?;
        let eager = count.min(EAGER_PAGES);
        let (mut pages, broken_pages) = read_page_sizes(&inner, 0..eager, None);
        let estimate = pages.last().copied().unwrap_or(FALLBACK_PAGE);
        pages.resize(count, estimate);
        let metadata = read_metadata(&inner);
        Ok(Self {
            inner,
            info: DocumentInfo {
                pages,
                metadata,
                broken_pages,
                measured_pages: u32::try_from(eager).unwrap_or(u32::MAX),
            },
        })
    }

    pub fn info(&self) -> &DocumentInfo {
        &self.info
    }

    pub fn page_count(&self) -> usize {
        self.info.pages.len()
    }

    /// Measures the next `chunk` estimated pages and records their exact
    /// sizes. Returns `None` once every page is measured.
    pub fn measure_next(&mut self, chunk: usize) -> Option<MeasuredPages> {
        if self.info.all_measured() {
            return None;
        }
        let first = self.info.measured_pages as usize;
        let end = (first + chunk.max(1)).min(self.info.pages.len());
        let previous = first
            .checked_sub(1)
            .and_then(|i| self.info.pages.get(i).copied());
        let (sizes, broken) = read_page_sizes(&self.inner, first..end, previous);
        self.info.pages[first..end].copy_from_slice(&sizes);
        self.info.broken_pages.extend_from_slice(&broken);
        self.info.measured_pages = u32::try_from(end).unwrap_or(u32::MAX);
        Some(MeasuredPages {
            first,
            sizes,
            broken,
        })
    }

    /// Optional content (layers) in display order; empty if there is none.
    pub fn layers(&self) -> Result<Vec<LayerInfo>, fitz::Error> {
        Ok(self
            .inner
            .layers()?
            .into_iter()
            .map(|l| LayerInfo {
                index: l.index,
                name: l.name,
                depth: l.depth,
                kind: match l.kind {
                    fitz::LayerKind::Label => LayerKind::Label,
                    fitz::LayerKind::Checkbox => LayerKind::Checkbox,
                    fitz::LayerKind::Radio => LayerKind::Radio,
                },
                visible: l.visible,
                locked: l.locked,
            })
            .collect())
    }

    /// Shows or hides a layer. Display lists built earlier are stale.
    pub fn set_layer(&mut self, index: u32, visible: bool) -> Result<(), fitz::Error> {
        self.inner.set_layer(index, visible)
    }

    /// Interprets a page into a display list (the expensive step of rendering).
    pub fn display_list(&self, page: usize) -> Result<DisplayList, fitz::Error> {
        let _span = info_span!("doc.display_list", page).entered();
        self.inner.load_page(page)?.to_display_list(true)
    }
}

fn read_page_sizes(
    doc: &fitz::Document,
    range: std::ops::Range<usize>,
    previous: Option<PageSize>,
) -> (Vec<PageSize>, Vec<u32>) {
    let _span = info_span!("doc.page_sizes", first = range.start, count = range.len()).entered();
    let mut sizes: Vec<PageSize> = Vec::with_capacity(range.len());
    let mut broken = Vec::new();
    for i in range {
        // Fast path reads the page tree only; loading the page (which also
        // parses annotations and links) is the fallback for non-PDF formats.
        let dims = match doc.page_size_fast(i) {
            Ok(Some(dims)) => Some(dims),
            _ => doc
                .load_page(i)
                .and_then(|p| p.bounds())
                .ok()
                .map(|b| (b.width(), b.height())),
        };
        let size = dims.and_then(|(w, h)| {
            (w.is_finite() && h.is_finite() && w >= 1.0 && h >= 1.0).then_some(PageSize {
                width: w,
                height: h,
            })
        });
        match size {
            Some(s) => sizes.push(s),
            None => {
                warn!(page = i, "unreadable page box, using fallback size");
                broken.push(i as u32);
                let fallback = sizes.last().copied().or(previous).unwrap_or(FALLBACK_PAGE);
                sizes.push(fallback);
            }
        }
    }
    (sizes, broken)
}

fn read_metadata(doc: &fitz::Document) -> Metadata {
    use fitz::MetadataKey as K;
    let get = |k| doc.metadata(k).ok().flatten();
    Metadata {
        title: get(K::Title),
        author: get(K::Author),
        subject: get(K::Subject),
        creator: get(K::Creator),
        producer: get(K::Producer),
        format: get(K::Format),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../bench/corpus/fixtures")
            .join(name)
    }

    #[test]
    fn opens_with_page_sizes_and_metadata() {
        let doc = Document::open(&fixture("paper-3.pdf"), None).unwrap();
        let info = doc.info();
        assert_eq!(
            info.pages,
            vec![
                PageSize {
                    width: 612.0,
                    height: 792.0
                };
                3
            ]
        );
        assert!(info.broken_pages.is_empty());
        assert_eq!(
            info.metadata.title.as_deref(),
            Some("Synthetic benchmark paper")
        );
        assert_eq!(info.metadata.producer.as_deref(), Some("corpusgen"));
        assert!(
            info.metadata
                .format
                .as_deref()
                .unwrap_or("")
                .starts_with("PDF")
        );
        assert!(doc.display_list(2).is_ok());
    }

    #[test]
    fn large_documents_are_measured_lazily() {
        let path = fixture("../generated/big-1000p.pdf");
        if !path.exists() {
            return; // needs `cargo run -p corpusgen --release`
        }
        let mut doc = Document::open(&path, None).unwrap();
        assert_eq!(doc.info().measured_pages as usize, EAGER_PAGES);
        assert_eq!(doc.page_count(), 1000);
        let mut next = EAGER_PAGES;
        while let Some(m) = doc.measure_next(300) {
            assert_eq!(m.first, next);
            next += m.sizes.len();
        }
        assert_eq!(next, 1000);
        assert!(doc.info().all_measured());
        assert!(doc.measure_next(10).is_none());
    }

    #[test]
    fn password_flow() {
        let path = fixture("encrypted-secret.pdf");
        assert!(matches!(
            Document::open(&path, None),
            Err(OpenError::PasswordRequired)
        ));
        assert!(matches!(
            Document::open(&path, Some("nope")),
            Err(OpenError::WrongPassword)
        ));
        assert_eq!(
            Document::open(&path, Some("secret")).unwrap().page_count(),
            3
        );
    }

    #[test]
    fn missing_file() {
        assert!(matches!(
            Document::open(&fixture("does-not-exist.pdf"), None),
            Err(OpenError::NotFound(_))
        ));
    }

    #[test]
    fn every_malformed_file_opens_or_fails_cleanly() {
        let dir = fixture("malformed");
        let mut seen = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            seen += 1;
            match Document::open(&path, None) {
                Ok(doc) => {
                    assert!(doc.page_count() > 0);
                    for p in &doc.info().pages {
                        assert!(p.width >= 1.0 && p.height >= 1.0, "{}", path.display());
                    }
                }
                Err(OpenError::Invalid(_) | OpenError::Empty) => {}
                Err(e) => panic!("{}: unexpected error {e}", path.display()),
            }
        }
        assert!(seen >= 10);
    }
}
