use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use xz2::read::XzDecoder;

#[derive(Clone, Debug)]
pub struct PatchEntry {
    pub name: String,
    pub enabled: bool,
    pub path: PathBuf,
}

/// Extended download result with metadata
#[derive(Clone, Debug)]
pub struct DownloadInfo {
    pub path: PathBuf,
    pub sha256: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

pub enum DownloadResult {
    Done(DownloadInfo),
    Error(String),
}

pub fn get_patch_dir(base_dir: &Path, kernel_series: &str) -> PathBuf {
    // e.g. linux6.13-tkg-userpatches
    let dir_name = format!("linux{}-tkg-userpatches", kernel_series);
    base_dir
        .join("submodules")
        .join("linux-tkg")
        .join(dir_name)
}

pub fn list_patches(patch_dir: &Path) -> Vec<PatchEntry> {
    let mut patches = Vec::new();

    if let Ok(entries) = fs::read_dir(patch_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".patch") || name.ends_with(".mypatch") {
                    patches.push(PatchEntry {
                        name: name.to_string(),
                        enabled: true,
                        path: path.clone(),
                    });
                } else if name.ends_with(".patch.disabled") || name.ends_with(".mypatch.disabled") {
                    patches.push(PatchEntry {
                        name: name.to_string(),
                        enabled: false,
                        path: path.clone(),
                    });
                }
            }
        }
    }

    patches.sort_by(|a, b| a.name.cmp(&b.name));
    patches
}

pub fn toggle_patch(patch: &mut PatchEntry) -> Result<(), String> {
    let new_path = if patch.enabled {
        // Disable: add .disabled suffix
        patch.path.with_extension(
            patch
                .path
                .extension()
                .map(|e| format!("{}.disabled", e.to_string_lossy()))
                .unwrap_or_else(|| "disabled".to_string()),
        )
    } else {
        // Enable: remove .disabled suffix
        let name = patch.path.to_string_lossy();
        PathBuf::from(name.trim_end_matches(".disabled"))
    };

    fs::rename(&patch.path, &new_path).map_err(|e| e.to_string())?;
    patch.path = new_path.clone();
    patch.name = new_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    patch.enabled = !patch.enabled;
    Ok(())
}

pub fn delete_patch(patch: &PatchEntry) -> Result<(), String> {
    fs::remove_file(&patch.path).map_err(|e| e.to_string())
}

pub fn download_patch(url: &str, dest_path: &Path) -> DownloadResult {
    match download_patch_inner(url, dest_path) {
        Ok(info) => DownloadResult::Done(info),
        Err(e) => DownloadResult::Error(e),
    }
}

fn download_patch_inner(url: &str, dest_path: &Path) -> Result<DownloadInfo, String> {
    // Ensure parent directory exists
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let response = ureq::get(url).call().map_err(|e| e.to_string())?;
    
    // Capture HTTP headers for update tracking
    let etag = response.header("ETag").map(|s| s.to_string());
    let last_modified = response.header("Last-Modified").map(|s| s.to_string());
    
    let mut reader = response.into_reader();

    // Check if file needs decompression based on extension
    let dest_str = dest_path.to_string_lossy();
    
    let (final_path, content) = if dest_str.ends_with(".xz") {
        // Decompress XZ and save without .xz extension
        let final_path = PathBuf::from(dest_str.trim_end_matches(".xz"));
        let mut compressed_data = Vec::new();
        reader.read_to_end(&mut compressed_data).map_err(|e| e.to_string())?;
        
        let mut decoder = XzDecoder::new(&compressed_data[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).map_err(|e| format!("XZ decompression failed: {}", e))?;
        
        (final_path, decompressed)
    } else if dest_str.ends_with(".gz") {
        // Decompress GZ and save without .gz extension
        let final_path = PathBuf::from(dest_str.trim_end_matches(".gz"));
        let mut compressed_data = Vec::new();
        reader.read_to_end(&mut compressed_data).map_err(|e| e.to_string())?;
        
        let mut decoder = GzDecoder::new(&compressed_data[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).map_err(|e| format!("GZ decompression failed: {}", e))?;
        
        (final_path, decompressed)
    } else {
        // No compression, read directly
        let mut content = Vec::new();
        reader.read_to_end(&mut content).map_err(|e| e.to_string())?;
        (dest_path.to_path_buf(), content)
    };
    
    // Compute SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let sha256 = format!("{:x}", hasher.finalize());
    
    // Write file
    fs::write(&final_path, &content).map_err(|e| e.to_string())?;
    
    Ok(DownloadInfo {
        path: final_path,
        sha256,
        etag,
        last_modified,
    })
}

pub fn extract_filename_from_url(url: &str) -> String {
    url.rsplit('/')
        .next()
        .unwrap_or("patch.patch")
        .to_string()
}
