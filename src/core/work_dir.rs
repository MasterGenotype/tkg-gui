use std::fs;
use std::path::{Path, PathBuf};

/// Manages a temporary working directory for all tkg-gui operations.
///
/// All mutable operations (cloning linux-tkg, downloading kernel sources,
/// building kernels) happen inside this directory. On drop, the directory
/// is removed unless `set_keep(true)` has been called — ensuring automatic
/// cleanup on panics/crashes while allowing the user to preserve files on
/// a normal exit.
pub struct WorkDir {
    path: PathBuf,
    keep: bool,
}

impl WorkDir {
    /// Create a new temporary working directory under the system temp dir.
    pub fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("tkg-gui-{}", std::process::id()));
        fs::create_dir_all(&path)
            .map_err(|e| format!("Failed to create work dir {}: {}", path.display(), e))?;
        Ok(Self { path, keep: false })
    }

    /// Root path of the temporary working directory.
    pub fn root(&self) -> &Path {
        &self.path
    }

    /// Path for the linux-tkg working copy.
    pub fn linux_tkg(&self) -> PathBuf {
        self.path.join("linux-tkg")
    }

    /// Path for downloaded kernel sources.
    pub fn kernel_sources(&self) -> PathBuf {
        self.path.join("kernel-sources")
    }

    /// If true, the work directory will be preserved when the app exits.
    pub fn set_keep(&mut self, keep: bool) {
        self.keep = keep;
    }

    /// Explicitly remove the working directory and all contents.
    pub fn cleanup(&self) -> Result<(), String> {
        if self.path.exists() {
            fs::remove_dir_all(&self.path)
                .map_err(|e| format!("Failed to clean up {}: {}", self.path.display(), e))?;
        }
        Ok(())
    }

    /// Returns true if a linux-tkg working copy is present with customization.cfg.
    pub fn is_linux_tkg_ready(&self) -> bool {
        self.linux_tkg().join("customization.cfg").exists()
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        if !self.keep && self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
