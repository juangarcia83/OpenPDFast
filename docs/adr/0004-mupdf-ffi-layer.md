# ADR 0004 — Own safe layer over mupdf-sys; optional content (spike S3)

- Status: accepted
- Date: 2026-09-25

## Context

ADR 0001 chose MuPDF through the `mupdf` crate (mupdf-rs 0.8). Spikes S1 and S3 found limits of that crate:

- **Optional content (S3).** mupdf-rs only edits the `/OCProperties /D` dictionary of the file
  (`set_optional_content_enabled`), which is an authoring feature. The viewer needs MuPDF's runtime layer
  state (`pdf_count_layer_config_ui`, `pdf_layer_config_ui_info`, `pdf_toggle_layer_config_ui`,
  `pdf_enable_layer`, …), which mupdf-sys exposes but mupdf-rs does not wrap, and mupdf-rs gives no access to
  raw pointers, so the two cannot be mixed.
- **Cancellation.** Aborting an in-flight render from another thread needs a shared `fz_cookie`;
  `mupdf::Cookie::abort` takes `&mut self`.
- **Context control.** S1 showed lock behaviour decides scaling; we want to own context creation.
- **Build on Windows.** mupdf-rs 0.8 does not compile on MSVC because bindgen never sees `max_align_t`. Fixed
  repo-wide in `.cargo/config.toml` + `.cargo/bindgen/max_align_t.h` (worth reporting upstream).

mupdf-sys ships C wrappers (`mupdf_*`) that run each call inside `fz_try` and return errors through an out
pointer; they cover everything F1 needs (open, password, metadata, pages, bounds, display lists with cookie,
display-list devices, pixmaps, draw device, outline, links, structured text).

## Decision

- New crate **`crates/fitz`**: a small, safe Rust layer over `mupdf-sys` (pinned `=0.8.0`) — context per thread,
  document, page, display list (`Send + Sync`), pixmap, cookie shareable across threads, errors via
  `thiserror`. All `unsafe` lives there, each block with `// SAFETY:`.
- `crates/doc` and `crates/render` depend on `fitz`, never on `mupdf-sys` directly. The `mupdf` crate is only
  used by spikes and the corpus generator.
- Only call MuPDF functions that cannot throw, or the `mupdf_*` wrappers. Functions that can throw and have no
  wrapper (layers in F2) get a tiny C shim compiled in `fitz` with `fz_try`.
- **OCG (F2):** toggle layers through MuPDF's layer-config UI API (`pdf_toggle_layer_config_ui` / `pdf_enable_layer`),
  then drop and rebuild the display lists of affected pages. The file itself is never modified.

## Consequences

- More code of our own (~a few hundred lines) in exchange for control over threading, cancellation and layers.
- mupdf-sys upgrades must be deliberate (`=` pin) and re-run the render benchmarks.
- AGENTS.md §3 gains `crates/fitz`.
