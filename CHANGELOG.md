# Changelog

All notable changes to this project are documented in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Cargo workspace with the `doc`, `render`, `pdf3d`, `latex`, `ink` and `app` crates.
- Tauri 2 shell with a SolidJS + TypeScript frontend, design tokens and English/Spanish i18n.
- Type-safe IPC bindings generated with `tauri-specta`.
- CI on GitHub Actions (fmt, clippy, Biome, tests, `cargo deny`, builds for Windows, macOS and Linux).
