use crate::core::config_manager::ConfigManager;
use std::path::{Path, PathBuf};

/// Returns the path to wine-tkg's customization.cfg.
/// The file lives inside the inner `wine-tkg-git/` subdirectory of the clone.
pub fn wine_config_path(wine_tkg_path: &Path) -> PathBuf {
    wine_tkg_path.join("wine-tkg-git").join("customization.cfg")
}

/// Load the wine-tkg customization.cfg. Returns an error string if the file
/// cannot be read or the path doesn't exist yet.
pub fn load(wine_tkg_path: &Path) -> Result<ConfigManager, String> {
    let path = wine_config_path(wine_tkg_path);
    ConfigManager::load(&path)
}
