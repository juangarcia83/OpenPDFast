// SPDX-License-Identifier: AGPL-3.0-or-later

//! Page rendering: the `PageRenderer` trait, backends, tiling, tile cache and scheduler.
//!
//! This crate must not depend on Tauri so it can be used from CLIs and tests.
