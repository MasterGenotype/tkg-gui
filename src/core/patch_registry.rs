use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::mpsc::Sender;

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum UpdateStatus {
    #[default]
    Unknown,
    UpToDate,
    Stale,
    CheckError(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PatchMeta {
    pub filename: String,
    pub kernel_series: String,
    pub source_url: Option<String>,
    pub catalog_id: Option<String>,
    pub sha256: String,
    pub downloaded_at: DateTime<Utc>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    #[serde(default)]
    pub update_status: UpdateStatus,
}

impl PatchMeta {
    pub fn key(&self) -> String {
        format!("{}/{}", self.kernel_series, self.filename)
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct PatchRegistry {
    /// Key: "<kernel_series>/<filename>", e.g., "6.13/pf-6.13.patch"
    pub patches: HashMap<String, PatchMeta>,
}

impl PatchRegistry {
    pub fn load(data_dir: &Path) -> Self {
        let registry_path = data_dir.join("patch_registry.json");
        if let Ok(content) = fs::read_to_string(&registry_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let registry_path = data_dir.join("patch_registry.json");
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&registry_path, content).map_err(|e| e.to_string())
    }

    pub fn record_download(&mut self, meta: PatchMeta) {
        let key = meta.key();
        self.patches.insert(key, meta);
    }

    pub fn remove(&mut self, series: &str, filename: &str) {
        let key = format!("{}/{}", series, filename);
        self.patches.remove(&key);
    }

    pub fn get(&self, series: &str, filename: &str) -> Option<&PatchMeta> {
        let key = format!("{}/{}", series, filename);
        self.patches.get(&key)
    }

    pub fn get_mut(&mut self, series: &str, filename: &str) -> Option<&mut PatchMeta> {
        let key = format!("{}/{}", series, filename);
        self.patches.get_mut(&key)
    }

    pub fn all_for_series(&self, series: &str) -> Vec<&PatchMeta> {
        self.patches
            .values()
            .filter(|m| m.kernel_series == series)
            .collect()
    }

    pub fn update_status(&mut self, series: &str, filename: &str, status: UpdateStatus) {
        if let Some(meta) = self.get_mut(series, filename) {
            meta.update_status = status;
        }
    }
}

/// Result of an update check
pub enum UpdateCheckResult {
    UpToDate { key: String },
    Stale { key: String },
    Error { key: String, reason: String },
    NoUrl { key: String },
}

/// Check if a patch has been updated at its source URL
/// Runs in a spawned thread
pub fn check_update(meta: PatchMeta, tx: Sender<UpdateCheckResult>) {
    std::thread::spawn(move || {
        let key = meta.key();

        let Some(url) = &meta.source_url else {
            let _ = tx.send(UpdateCheckResult::NoUrl { key });
            return;
        };

        let result = ureq::head(url).call();

        match result {
            Ok(response) => {
                let new_etag = response.header("ETag").map(|s| s.to_string());
                let new_last_modified = response.header("Last-Modified").map(|s| s.to_string());

                // Check if headers changed
                let etag_changed = match (&meta.etag, &new_etag) {
                    (Some(old), Some(new)) => old != new,
                    (None, Some(_)) => true,
                    _ => false,
                };

                let modified_changed = match (&meta.last_modified, &new_last_modified) {
                    (Some(old), Some(new)) => old != new,
                    (None, Some(_)) => true,
                    _ => false,
                };

                if etag_changed || modified_changed {
                    let _ = tx.send(UpdateCheckResult::Stale {
                        key,
                    });
                } else {
                    let _ = tx.send(UpdateCheckResult::UpToDate { key });
                }
            }
            Err(e) => {
                let _ = tx.send(UpdateCheckResult::Error {
                    key,
                    reason: e.to_string(),
                });
            }
        }
    });
}
