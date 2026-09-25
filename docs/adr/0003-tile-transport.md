# ADR 0003 — Tile transport from Rust to the webview (spike S2)

- Status: accepted
- Date: 2026-09-25
- Spike: `spikes/s2-tile-transport` (`cargo run -p s2-tile-transport --release`; the page benchmarks itself,
  prints JSON and exits)
- Hardware: [`bench/HARDWARE.md`](../../bench/HARDWARE.md); WebView2 153 on Windows 11.

## Question

Can the webview receive raw RGBA tiles fast enough, or do we need the native wgpu viewport (AGENTS.md §14.1)?
Acceptance threshold from `docs/PLAN.md`: ~60 tiles/s of 512×512.

## Method

240 distinct pre-rendered 512×512 RGBA tiles (1 MiB each, 8-byte header), no encoding. Transports:
`invoke` returning `tauri::ipc::Response` (pull), `tauri::ipc::Channel<Response>` (push) and a custom `tile://`
URI scheme read with `fetch` (pull). "Paint" = `new ImageData(view)` → `createImageBitmap` → `drawImage`.

## Results

| Transport | tiles/s | MB/s |
|---|---|---|
| invoke, 1 in flight | 105 | 111 |
| invoke, 4 in flight | 176 | 184 |
| invoke, 8 in flight | 181 | 189 |
| **Channel (push)** | **186** | **195** |
| fetch `tile://`, 1 / 4 / 8 in flight | 105 / 177 / 185 | 110 / 186 / 194 |
| invoke ×4 + paint | 160 | 168 |
| **Channel + paint** | **172** | **180** |
| fetch ×4 + paint | 144 | 151 |

Latency of one tile, request → painted: p50 10.7 ms, p95 11.7 ms (invoke); 10.5 / 11.4 ms (fetch).

## Decision

- The webview path passes with ~3× margin (≈ 170 painted tiles/s). **The wgpu native viewport is not needed**
  for F1; §14.1 stays open only for F2 if plans prove otherwise.
- Use one `tauri::ipc::Channel` per document to **push** tiles as soon as they are rendered (fastest option,
  and the scheduler — not the frontend — decides order and cancellation). Each message = small binary header
  (tile key, generation, size) + raw RGBA bytes.
- Frontend: wrap the bytes in `ImageData` without copying and upload with `createImageBitmap`.
- The ~1 MB-per-tile cost (≈ 5.5 ms of transport each) means we must not resend tiles: the frontend keeps the
  tiles it has, and the backend only sends what the viewport lacks.
