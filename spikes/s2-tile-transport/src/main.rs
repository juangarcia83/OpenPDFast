// SPDX-License-Identifier: AGPL-3.0-or-later

//! Spike S2: how fast can raw RGBA tiles travel from Rust to the webview?
//!
//! Three transports are compared, all without encoding (no JSON, no PNG):
//! - `invoke` returning `tauri::ipc::Response` (pull, one call per tile);
//! - `tauri::ipc::Channel` (push, Rust streams tiles as they are ready);
//! - a custom `tile://` URI scheme read with `fetch` (pull).
//!
//! The page runs the benchmark by itself, reports JSON back and the app exits.

use std::sync::Arc;

use tauri::Manager;
use tauri::http::Response as HttpResponse;
use tauri::ipc::{Channel, Response};

const TILE: usize = 512;
const BYTES: usize = TILE * TILE * 4;

/// A pool of distinct pre-rendered tiles so generation cost is excluded.
struct Tiles(Vec<Arc<Vec<u8>>>);

impl Tiles {
    fn new() -> Self {
        let tiles = (0..16u8)
            .map(|k| {
                let mut v = vec![0u8; BYTES];
                for (i, px) in v.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let (x, y) = (i % TILE, i / TILE);
                    *px = [
                        (x as u8).wrapping_add(k * 16),
                        y as u8,
                        k.wrapping_mul(40),
                        255,
                    ];
                }
                Arc::new(v)
            })
            .collect();
        Self(tiles)
    }

    /// Tile bytes prefixed by an 8-byte header (index, byte length), like
    /// the real transport will prefix the tile key.
    fn framed(&self, i: u32) -> Vec<u8> {
        let tile = &self.0[i as usize % self.0.len()];
        let mut out = Vec::with_capacity(8 + BYTES);
        out.extend_from_slice(&i.to_le_bytes());
        out.extend_from_slice(&(BYTES as u32).to_le_bytes());
        out.extend_from_slice(tile);
        out
    }
}

#[tauri::command]
fn tile_response(i: u32, tiles: tauri::State<'_, Tiles>) -> Response {
    Response::new(tiles.framed(i))
}

#[tauri::command]
async fn tile_stream(
    count: u32,
    channel: Channel<Response>,
    tiles: tauri::State<'_, Tiles>,
) -> Result<(), String> {
    for i in 0..count {
        channel
            .send(Response::new(tiles.framed(i)))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn report(app: tauri::AppHandle, results: serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(&results).unwrap_or_default()
    );
    app.exit(0);
}

fn main() {
    let result = tauri::Builder::default()
        .manage(Tiles::new())
        .register_asynchronous_uri_scheme_protocol("tile", |ctx, req, responder| {
            let i: u32 = req
                .uri()
                .path()
                .trim_start_matches('/')
                .parse()
                .unwrap_or(0);
            let body = ctx.app_handle().state::<Tiles>().framed(i);
            responder.respond(
                HttpResponse::builder()
                    .header("Content-Type", "application/octet-stream")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(body)
                    .unwrap_or_default(),
            );
        })
        .invoke_handler(tauri::generate_handler![tile_response, tile_stream, report])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("{e}");
    }
}
