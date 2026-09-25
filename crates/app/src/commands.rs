// SPDX-License-Identifier: AGPL-3.0-or-later

//! IPC commands. Keep them thin: validate input, delegate to the core crates.

use serde::Serialize;
use specta::Type;

/// Static information about the application, shown in the About screen.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub license: String,
    /// Public source code repository, linked from the UI as the AGPL requires.
    pub source_url: String,
}

#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "PDF Reader".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        license: env!("CARGO_PKG_LICENSE").to_owned(),
        source_url: env!("CARGO_PKG_REPOSITORY").to_owned(),
    }
}
