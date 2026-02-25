use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Progress update sent during download/extraction
#[derive(Clone, Debug)]
pub enum DownloadProgress {
    /// Download started, contains total size in bytes (if known)
    Started(Option<u64>),
    /// Downloaded bytes so far
    Downloading(u64),
    /// Download complete, starting extraction
    Extracting,
    /// Extraction complete, path to extracted folder
    Complete(PathBuf),
    /// Error occurred
    Error(String),
}

/// Result of a download operation
pub enum DownloadResult {
    Success,
    Error,
}

/// Get the download URL for a kernel version
/// e.g., "6.19.2" -> "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.19.2.tar.xz"
pub fn get_download_url(version: &str) -> String {
    let version = version.trim_start_matches('v');
    let major = version.split('.').next().unwrap_or("6");
    format!(
        "https://cdn.kernel.org/pub/linux/kernel/v{}.x/linux-{}.tar.xz",
        major, version
    )
}

/// Download and extract kernel sources
///
/// # Arguments
/// * `version` - Kernel version (e.g., "6.19.2" or "v6.19.2")
/// * `dest_dir` - Destination directory for extracted sources
/// * `tx` - Channel sender for progress updates
pub fn download_kernel(
    version: &str,
    dest_dir: &Path,
    tx: std::sync::mpsc::Sender<DownloadProgress>,
) -> DownloadResult {
    let url = get_download_url(version);
    let version = version.trim_start_matches('v');

    // Create destination directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(dest_dir) {
        let msg = format!("Failed to create destination directory: {}", e);
        let _ = tx.send(DownloadProgress::Error(msg));
        return DownloadResult::Error;
    }

    let tarball_path = dest_dir.join(format!("linux-{}.tar.xz", version));

    // Download the tarball
    match download_file(&url, &tarball_path, &tx) {
        Ok(()) => {}
        Err(e) => {
            let _ = tx.send(DownloadProgress::Error(e));
            return DownloadResult::Error;
        }
    }

    // Extract the tarball
    let _ = tx.send(DownloadProgress::Extracting);
    match extract_tarball(&tarball_path, dest_dir) {
        Ok(extracted_path) => {
            // Clean up tarball after successful extraction
            let _ = fs::remove_file(&tarball_path);
            let _ = tx.send(DownloadProgress::Complete(extracted_path));
            DownloadResult::Success
        }
        Err(e) => {
            let _ = tx.send(DownloadProgress::Error(e));
            DownloadResult::Error
        }
    }
}

/// Download a file with progress updates
fn download_file(
    url: &str,
    dest: &Path,
    tx: &std::sync::mpsc::Sender<DownloadProgress>,
) -> Result<(), String> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("Failed to download: {}", e))?;

    let total_size = response
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok());

    let _ = tx.send(DownloadProgress::Started(total_size));

    let mut reader = response.into_reader();
    let mut file = File::create(dest)
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| format!("Failed to read: {}", e))?;

        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])
            .map_err(|e| format!("Failed to write: {}", e))?;

        downloaded += bytes_read as u64;
        let _ = tx.send(DownloadProgress::Downloading(downloaded));
    }

    Ok(())
}

/// Extract a .tar.xz tarball
fn extract_tarball(tarball: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let file = File::open(tarball)
        .map_err(|e| format!("Failed to open tarball: {}", e))?;

    // Decompress XZ
    let decompressor = xz2::read::XzDecoder::new(file);

    // Extract tar
    let mut archive = tar::Archive::new(decompressor);

    archive
        .unpack(dest_dir)
        .map_err(|e| format!("Failed to extract tarball: {}", e))?;

    // Find the extracted directory (should be linux-X.Y.Z)
    let filename = tarball
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("linux");
    let extracted_name = filename.trim_end_matches(".tar");
    let extracted_path = dest_dir.join(extracted_name);

    if extracted_path.exists() {
        Ok(extracted_path)
    } else {
        // Try to find any linux-* directory
        for entry in fs::read_dir(dest_dir).map_err(|e| e.to_string())?.flatten() {
            let name = entry.file_name();
            if let Some(name_str) = name.to_str() {
                if name_str.starts_with("linux-") && entry.path().is_dir() {
                    return Ok(entry.path());
                }
            }
        }
        Err("Could not find extracted kernel directory".to_string())
    }
}

/// Format bytes as human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
