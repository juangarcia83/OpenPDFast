// SPDX-License-Identifier: AGPL-3.0-or-later

//! Open documents and their render schedulers, plus the IPC commands on them.
//!
//! Tiles are pushed to the frontend over one `Channel` per document as raw
//! bytes (ADR 0003). Message layout, little-endian:
//!
//! | offset | tile (`kind = 1`)           | page failure (`kind = 2`) |
//! |--------|-----------------------------|---------------------------|
//! | 0      | `u32` kind                  | `u32` kind                |
//! | 4      | `u32` page                  | `u32` page                |
//! | 8      | `i32` zoom level            | UTF-8 message …           |
//! | 12     | `u32` tile x                |                           |
//! | 16     | `u32` tile y                |                           |
//! | 20     | `u32` width                 |                           |
//! | 24     | `u32` height                |                           |
//! | 28     | RGBA pixels, `width*height*4` |                         |

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

use doc::{Document, DocumentInfo, OpenError};
use parking_lot::Mutex;
use render::{MupdfRenderer, RenderEvent, Scheduler, SchedulerConfig, Viewport};
use serde::Serialize;
use specta::Type;
use tauri::State;
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};
use tracing::{info, info_span, warn};

use crate::view_state::{ViewState, ViewStates};

pub const MSG_TILE: u32 = 1;
pub const MSG_PAGE_FAILED: u32 = 2;
const TILE_HEADER: usize = 28;

/// One raw channel message (layout in the module docs). Sent without any
/// encoding; arrives in JavaScript as an `ArrayBuffer`.
pub struct TileMessage(Vec<u8>);

impl IpcResponse for TileMessage {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

impl specta::Type for TileMessage {
    fn definition(_: &mut specta::Types) -> specta::datatype::DataType {
        specta::datatype::DataType::Reference(specta_typescript::define("ArrayBuffer"))
    }
}

struct Session {
    scheduler: Scheduler<MupdfRenderer>,
    path: PathBuf,
}

#[derive(Default)]
pub struct Documents {
    next_id: AtomicU32,
    open: Mutex<HashMap<u32, Arc<Session>>>,
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenedDocument {
    pub id: u32,
    pub path: String,
    pub file_name: String,
    pub info: DocumentInfo,
    /// Where the user left this document last time, if known.
    pub view_state: Option<ViewState>,
}

/// Errors shown to the user. Messages never include passwords.
#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum DocumentError {
    #[error("file not found")]
    NotFound,
    #[error("password required")]
    PasswordRequired,
    #[error("wrong password")]
    WrongPassword,
    #[error("the document has no pages")]
    Empty,
    #[error("invalid document: {0}")]
    Invalid(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("unknown document id")]
    UnknownDocument,
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<OpenError> for DocumentError {
    fn from(e: OpenError) -> Self {
        match e {
            OpenError::NotFound(_) => Self::NotFound,
            OpenError::PasswordRequired => Self::PasswordRequired,
            OpenError::WrongPassword => Self::WrongPassword,
            OpenError::Empty => Self::Empty,
            OpenError::Invalid(m) => Self::Invalid(m),
            OpenError::Io(e) => Self::Io(e.to_string()),
        }
    }
}

fn encode_event(event: &RenderEvent) -> Vec<u8> {
    match event {
        RenderEvent::Tile(tile) => {
            let img = &tile.image;
            let mut out = Vec::with_capacity(TILE_HEADER + img.pixels.len());
            for v in [MSG_TILE, tile.key.page] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&i32::from(tile.key.level.0).to_le_bytes());
            for v in [tile.key.x, tile.key.y, img.width, img.height] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&img.pixels);
            out
        }
        RenderEvent::PageFailed { page, message } => {
            let mut out = Vec::with_capacity(8 + message.len());
            out.extend_from_slice(&MSG_PAGE_FAILED.to_le_bytes());
            out.extend_from_slice(&page.to_le_bytes());
            out.extend_from_slice(message.as_bytes());
            out
        }
    }
}

/// Opens a document and starts streaming its tiles over `tiles` as soon as a
/// viewport is set. Parsing runs on a blocking thread, never on the UI thread.
#[tauri::command]
#[specta::specta]
pub async fn open_document(
    path: String,
    password: Option<String>,
    tiles: Channel<TileMessage>,
    documents: State<'_, Documents>,
    view_states: State<'_, ViewStates>,
) -> Result<OpenedDocument, DocumentError> {
    let started = Instant::now();
    let path_buf = PathBuf::from(&path);
    let open_path = path_buf.clone();
    let doc = tauri::async_runtime::spawn_blocking(move || {
        Document::open(&open_path, password.as_deref())
    })
    .await
    .map_err(|e| DocumentError::Internal(e.to_string()))??;
    let info = doc.info().clone();

    let first_sent = AtomicBool::new(false);
    let sink = Arc::new(move |event: RenderEvent| {
        let _span = info_span!("render.send").entered();
        if !first_sent.swap(true, Ordering::Relaxed) {
            info!(
                elapsed_ms = started.elapsed().as_secs_f64() * 1e3,
                "first tile sent"
            );
        }
        if let Err(e) = tiles.send(TileMessage(encode_event(&event))) {
            warn!(error = %e, "tile channel closed");
        }
    });
    let scheduler = Scheduler::new(MupdfRenderer::new(doc), SchedulerConfig::default(), sink)
        .map_err(|e| DocumentError::Internal(e.to_string()))?;

    let id = documents.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    documents.open.lock().insert(
        id,
        Arc::new(Session {
            scheduler,
            path: path_buf.clone(),
        }),
    );
    info!(
        id,
        pages = info.pages.len(),
        elapsed_ms = started.elapsed().as_secs_f64() * 1e3,
        "document opened"
    );

    let file_name = path_buf
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    let view_state = view_states.get(&path_buf).await;
    Ok(OpenedDocument {
        id,
        path,
        file_name,
        info,
        view_state,
    })
}

fn session(documents: &Documents, id: u32) -> Result<Arc<Session>, DocumentError> {
    documents
        .open
        .lock()
        .get(&id)
        .cloned()
        .ok_or(DocumentError::UnknownDocument)
}

/// Tells the renderer what is on screen; it decides which tiles to send.
#[tauri::command]
#[specta::specta]
pub async fn set_viewport(
    id: u32,
    viewport: Viewport,
    documents: State<'_, Documents>,
) -> Result<(), DocumentError> {
    session(&documents, id)?.scheduler.set_viewport(viewport);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn close_document(id: u32, documents: State<'_, Documents>) -> Result<(), DocumentError> {
    // Dropping the session cancels in-flight work and stops its threads.
    documents
        .open
        .lock()
        .remove(&id)
        .ok_or(DocumentError::UnknownDocument)?;
    Ok(())
}

/// Remembers where the user is in a document (called debounced by the UI).
#[tauri::command]
#[specta::specta]
pub async fn save_view_state(
    id: u32,
    state: ViewState,
    documents: State<'_, Documents>,
    view_states: State<'_, ViewStates>,
) -> Result<(), DocumentError> {
    let path = session(&documents, id)?.path.clone();
    view_states
        .put(&path, state)
        .await
        .map_err(|e| DocumentError::Io(e.to_string()))
}

/// Files passed on the command line, to open at startup.
#[tauri::command]
#[specta::specta]
pub fn startup_files() -> Vec<String> {
    std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use render::{RenderedTile, RgbaImage, TileKey, ZoomLevel};

    use super::*;

    #[test]
    fn tile_messages_have_a_fixed_header() {
        let image = Arc::new(RgbaImage {
            width: 2,
            height: 1,
            pixels: vec![9; 8],
        });
        let key = TileKey {
            page: 7,
            level: ZoomLevel(-3),
            x: 1,
            y: 2,
        };
        let msg = encode_event(&RenderEvent::Tile(RenderedTile { key, image }));
        assert_eq!(msg.len(), TILE_HEADER + 8);
        let word = |i: usize| u32::from_le_bytes(msg[i..i + 4].try_into().unwrap());
        assert_eq!(
            [word(0), word(4), word(12), word(16), word(20), word(24)],
            [MSG_TILE, 7, 1, 2, 2, 1]
        );
        assert_eq!(i32::from_le_bytes(msg[8..12].try_into().unwrap()), -3);
        assert_eq!(&msg[TILE_HEADER..], &[9; 8]);
    }

    #[test]
    fn failure_messages_carry_the_text() {
        let msg = encode_event(&RenderEvent::PageFailed {
            page: 3,
            message: "bad".into(),
        });
        assert_eq!(&msg[..4], &MSG_PAGE_FAILED.to_le_bytes());
        assert_eq!(&msg[4..8], &3u32.to_le_bytes());
        assert_eq!(&msg[8..], b"bad");
    }
}
