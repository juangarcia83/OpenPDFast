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
    /// The path as the user opened it (the key is normalized).
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    page_count: Option<u32>,
}

/// A document the user opened before, for the start screen.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentDocument {
    pub path: String,
    pub file_name: String,
    /// Zero-based page where the user left off.
    pub page: u32,
    pub page_count: Option<u32>,
    /// Seconds since the Unix epoch (u32 is enough until 2106 and stays
    /// a plain JS number).
    pub last_opened: u32,
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

    /// Most recently used documents that still exist, newest first.
    pub async fn recent(&self, limit: usize) -> Vec<RecentDocument> {
        let mut entries = self.entries.lock().await;
        self.load(&mut entries).await;
        let mut list: Vec<_> = entries
            .iter()
            .flat_map(|m| m.values())
            .filter_map(|e| {
                let path = e.path.as_ref()?;
                path.is_file().then(|| RecentDocument {
                    path: path.to_string_lossy().into_owned(),
                    file_name: path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    page: e.state.page,
                    page_count: e.page_count,
                    last_opened: u32::try_from(e.used).unwrap_or(u32::MAX),
                })
            })
            .collect();
        list.sort_by_key(|d| std::cmp::Reverse(d.last_opened));
        list.truncate(limit);
        list
    }

    /// Forgets a document (it disappears from the recent list).
    pub async fn remove(&self, path: &Path) -> std::io::Result<()> {
        let mut entries = self.entries.lock().await;
        self.load(&mut entries).await;
        if let Some(map) = entries.as_mut() {
            map.remove(&key(path));
        }
        self.save(entries.as_ref()).await
    }

    pub async fn put(&self, path: &Path, state: ViewState, page_count: u32) -> std::io::Result<()> {
        let mut entries = self.entries.lock().await;
        self.load(&mut entries).await;
        let map = entries.get_or_insert_with(HashMap::new);
        let used = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        map.insert(
            key(path),
            Entry {
                state,
                used,
                path: Some(path.to_owned()),
                page_count: Some(page_count),
            },
        );
        if map.len() > MAX_ENTRIES {
            let mut by_age: Vec<_> = map.iter().map(|(k, e)| (e.used, k.clone())).collect();
            by_age.sort_unstable();
            for (_, k) in by_age.into_iter().take(map.len() - MAX_ENTRIES) {
                map.remove(&k);
            }
        }
        self.save(Some(map)).await
    }

    async fn save(&self, map: Option<&HashMap<String, Entry>>) -> std::io::Result<()> {
        let Some(map) = map else { return Ok(()) };
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
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&doc, b"%PDF").unwrap();
        states.put(&doc, state(4), 9).await.unwrap();
        let reloaded = ViewStates::new(file);
        assert_eq!(reloaded.get(&doc).await, Some(state(4)));
        let recent = reloaded.recent(5).await;
        assert_eq!(recent.len(), 1);
        assert_eq!(
            (
                recent[0].file_name.as_str(),
                recent[0].page,
                recent[0].page_count
            ),
            ("a.pdf", 4, Some(9))
        );
        reloaded.remove(&doc).await.unwrap();
        assert!(reloaded.recent(5).await.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
