// SPDX-License-Identifier: AGPL-3.0-or-later

//! Last reading position per document, stored as JSON in the app data dir.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::Mutex;

/// Oldest entries are dropped beyond this many documents.
const MAX_ENTRIES: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FitMode {
    Width,
    Page,
    Free,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    /// Page at the top of the window.
    pub page: u32,
    /// How far into that page the top of the window is, from 0 to 1.
    pub page_offset: f32,
    /// Horizontal scroll as a fraction of the scrollable width, from 0 to 1.
    pub scroll_x: f32,
    /// Zoom where 1 = 100 % (1 pt = 1/72 in at 96 dpi).
    pub zoom: f32,
    pub fit: FitMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    state: ViewState,
    /// Seconds since the Unix epoch; used to drop the oldest entries.
    used: u64,
}

pub struct ViewStates {
    file: PathBuf,
    entries: Mutex<Option<HashMap<String, Entry>>>,
}

fn key(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    canonical.to_string_lossy().to_lowercase()
}

impl ViewStates {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            entries: Mutex::new(None),
        }
    }

    async fn load(&self, slot: &mut Option<HashMap<String, Entry>>) {
        if slot.is_none() {
            let loaded = tokio::fs::read(&self.file)
                .await
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            *slot = Some(loaded);
        }
    }

    pub async fn get(&self, path: &Path) -> Option<ViewState> {
        let mut entries = self.entries.lock().await;
        self.load(&mut entries).await;
        entries.as_ref()?.get(&key(path)).map(|e| e.state.clone())
    }

    pub async fn put(&self, path: &Path, state: ViewState) -> std::io::Result<()> {
        let mut entries = self.entries.lock().await;
        self.load(&mut entries).await;
        let map = entries.get_or_insert_with(HashMap::new);
        let used = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        map.insert(key(path), Entry { state, used });
        if map.len() > MAX_ENTRIES {
            let mut by_age: Vec<_> = map.iter().map(|(k, e)| (e.used, k.clone())).collect();
            by_age.sort_unstable();
            for (_, k) in by_age.into_iter().take(map.len() - MAX_ENTRIES) {
                map.remove(&k);
            }
        }
        let json = serde_json::to_vec(map).map_err(std::io::Error::other)?;
        if let Some(dir) = self.file.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        // Write-then-rename so a crash never leaves a truncated file.
        let tmp = self.file.with_extension("json.tmp");
        tokio::fs::write(&tmp, json).await?;
        tokio::fs::rename(&tmp, &self.file).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(page: u32) -> ViewState {
        ViewState {
            page,
            page_offset: 0.5,
            scroll_x: 0.0,
            zoom: 1.25,
            fit: FitMode::Free,
        }
    }

    #[tokio::test]
    async fn persists_and_reloads() {
        let dir = std::env::temp_dir().join(format!("pdfr-view-state-{}", std::process::id()));
        let file = dir.join("view-states.json");
        let doc = dir.join("a.pdf");
        let states = ViewStates::new(file.clone());
        assert_eq!(states.get(&doc).await, None);
        states.put(&doc, state(4)).await.unwrap();
        assert_eq!(ViewStates::new(file).get(&doc).await, Some(state(4)));
        let _ = std::fs::remove_dir_all(dir);
    }
}
