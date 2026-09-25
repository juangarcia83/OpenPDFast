# Changelog

All notable changes to this project are documented in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Cargo workspace with the `doc`, `render`, `pdf3d`, `latex`, `ink` and `app` crates.
- Tauri 2 shell with a SolidJS + TypeScript frontend, design tokens and English/Spanish i18n.
- Type-safe IPC bindings generated with `tauri-specta`.
- CI on GitHub Actions (fmt, clippy, Biome, tests, `cargo deny`, builds for Windows, macOS and Linux).
- Spikes S1–S3 with ADRs 0002–0004 and a deterministic synthetic benchmark corpus (`corpusgen`).
- `fitz`: safe layer over mupdf-sys (per-thread contexts, display lists, cross-thread cancellation).
- `doc`: open documents (with password), page sizes, metadata, typed errors.
- `render`: tile pyramid, `PageRenderer` + MuPDF backend, prioritized scheduler with cancellation, LRU caches,
  criterion benchmarks.
- Viewer: continuous scrolling, progressive tiles, zoom anchored to the cursor, fit width/page, open from
  dialog, drag and drop or command line, remembered position per document.
