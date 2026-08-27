//! Live browser for the community patch repository
//! ([`sirlucjan/kernel-patches`](https://github.com/sirlucjan/kernel-patches)).
//!
//! Mirrors the kernel-version browser: fetch the list of patchsets available
//! for the selected kernel series over the GitHub contents API, then download
//! a whole patchset with one click. Downloaded files are always written with
//! the `.mypatch` extension so linux-tkg's userpatch mechanism actually applies
//! them (it globs `*.mypatch`, ignoring plain `.patch`).

use crate::core::http_client;
use crate::core::patch_manager::{download_patch, DownloadInfo, DownloadResult};
use serde::Deserialize;
use std::path::Path;

const USER_AGENT: &str = "tkg-gui";

/// GitHub contents-API URL for a repo-relative path, e.g.
/// `contents_url("sirlucjan/kernel-patches", "7.2")`.
fn contents_url(repo: &str, path: &str) -> String {
    format!("https://api.github.com/repos/{}/contents/{}", repo, path)
}

/// A patchset directory available in the remote repo for a kernel series.
#[derive(Clone, Debug)]
pub struct RemotePatchset {
    /// Directory name, e.g. "handheld-patches".
    pub name: String,
    /// Repo-relative path, e.g. "7.2/handheld-patches".
    pub path: String,
}

pub enum BrowseResult {
    Done(Vec<RemotePatchset>),
    Error(String),
}

/// One file installed as a result of downloading a patchset.
#[derive(Clone, Debug)]
pub struct InstalledPatch {
    pub filename: String,
    pub source_url: String,
    pub info: DownloadInfo,
}

pub enum PatchsetResult {
    Done(Vec<InstalledPatch>),
    Error(String),
}

#[derive(Deserialize)]
struct GhEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
    download_url: Option<String>,
}

fn get_contents(url: &str) -> Result<Vec<GhEntry>, String> {
    let response = http_client::agent()
        .get(url)
        // GitHub rejects API requests without a User-Agent.
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(403, _) => "GitHub API rate limit reached \
                 (60 requests/hour without authentication). Try again later."
                .to_string(),
            ureq::Error::Status(404, _) => {
                "No patches found for this kernel series in sirlucjan/kernel-patches.".to_string()
            }
            other => other.to_string(),
        })?;

    let body = response.into_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

/// Fetch the patchset directories available for a kernel series (e.g. "7.2")
/// from `repo` (a GitHub `owner/repo`).
pub fn browse_patchsets(repo: &str, series: &str) -> BrowseResult {
    let url = contents_url(repo, series);
    match get_contents(&url) {
        Ok(entries) => {
            let mut sets: Vec<RemotePatchset> = entries
                .into_iter()
                .filter(|e| e.entry_type == "dir")
                .map(|e| RemotePatchset {
                    name: e.name,
                    path: e.path,
                })
                .collect();
            sets.sort_by(|a, b| a.name.cmp(&b.name));
            BrowseResult::Done(sets)
        }
        Err(e) => BrowseResult::Error(e),
    }
}

/// Download every patch file in a patchset directory into `patch_dir`, forcing
/// the `.mypatch` extension. Files keep their upstream ordering (numeric
/// prefixes) and are namespaced by the patchset directory to avoid collisions.
pub fn download_patchset(
    repo: &str,
    path: &str,
    dir_name: &str,
    patch_dir: &Path,
) -> PatchsetResult {
    let url = contents_url(repo, path);
    let entries = match get_contents(&url) {
        Ok(e) => e,
        Err(e) => return PatchsetResult::Error(e),
    };

    let files: Vec<GhEntry> = entries
        .into_iter()
        .filter(|e| e.entry_type == "file")
        .filter(|e| {
            let n = e.name.to_lowercase();
            n.ends_with(".patch") || n.ends_with(".diff")
        })
        .collect();

    if files.is_empty() {
        return PatchsetResult::Error(format!("No patch files found in {}", path));
    }

    let mut installed = Vec::new();
    for entry in files {
        let Some(dl) = entry.download_url.clone() else {
            continue;
        };
        let out_name = mypatch_name(dir_name, &entry.name);
        let dest = patch_dir.join(&out_name);
        match download_patch(&dl, &dest) {
            DownloadResult::Done(info) => installed.push(InstalledPatch {
                filename: out_name,
                source_url: dl,
                info,
            }),
            DownloadResult::Error(e) => {
                return PatchsetResult::Error(format!("{}: {}", entry.name, e));
            }
        }
    }

    PatchsetResult::Done(installed)
}

/// Build a `.mypatch` filename: `<dir>-<original-stem>.mypatch`.
fn mypatch_name(dir_name: &str, file_name: &str) -> String {
    let stem = file_name
        .strip_suffix(".patch")
        .or_else(|| file_name.strip_suffix(".diff"))
        .unwrap_or(file_name);
    format!("{}-{}.mypatch", dir_name, stem)
}

/// Short human hint for a patchset, matched on well-known directory names.
pub fn describe_patchset(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    let hint = if n.contains("handheld") {
        "Handheld/console device support (Steam Deck, Legion Go, ROG Ally)"
    } else if n.contains("amd-pstate") {
        "AMD P-State CPUFreq driver improvements"
    } else if n.contains("bbr3") {
        "Google BBRv3 TCP congestion control"
    } else if n.contains("bore") {
        "BORE scheduler — prefer the _cpusched config option over patching"
    } else if n.contains("prjc") {
        "Project C / BMQ-PDS scheduler"
    } else if n.contains("gaming-sched") {
        "Gaming-tuned scheduler tweaks"
    } else if n.starts_with("ck-") || n == "ck-patches" {
        "Con Kolivas desktop-latency patches"
    } else if n.contains("cachyos") {
        "CachyOS base patchset — may conflict with linux-tkg's own patches"
    } else if n.contains("zstd") {
        "Upstream ZSTD compression updates"
    } else if n.contains("ksm") {
        "Kernel Samepage Merging tweaks"
    } else if n.starts_with("t2") {
        "Apple T2 Mac hardware support"
    } else if n.starts_with("rt-") {
        "Realtime (PREEMPT_RT) patches"
    } else if n.contains("s5-power") {
        "S5 power / suspend-to-idle fixes"
    } else if n.contains("adios") {
        "ADIOS I/O scheduler"
    } else if n.contains("block") {
        "Block layer / I/O tweaks"
    } else if n.contains("clang") {
        "Clang/LLVM build fixes"
    } else {
        return None;
    };
    Some(hint)
}
