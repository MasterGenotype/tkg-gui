use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
    } else {
        PathBuf::from(".")
    }
}

fn default_linux_tkg_path() -> PathBuf {
    home_dir()
        .join(".local")
        .join("share")
        .join("tkg-gui")
        .join("linux-tkg")
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AppSettings {
    #[serde(default = "default_linux_tkg_path")]
    pub linux_tkg_path: PathBuf,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            linux_tkg_path: default_linux_tkg_path(),
        }
    }
}

impl AppSettings {
    /// Directory for app configuration files: ~/.config/tkg-gui/
    pub fn config_dir() -> PathBuf {
        home_dir().join(".config").join("tkg-gui")
    }

    /// Directory for app data files (patch registry, etc.): ~/.local/share/tkg-gui/
    pub fn data_dir() -> PathBuf {
        home_dir().join(".local").join("share").join("tkg-gui")
    }

    pub fn load() -> Self {
        let path = Self::config_dir().join("settings.json");
        if let Ok(content) = fs::read_to_string(&path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::config_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("settings.json");
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, content).map_err(|e| e.to_string())
    }

    /// Returns true if linux-tkg appears to be cloned at linux_tkg_path
    pub fn is_cloned(&self) -> bool {
        self.linux_tkg_path.join("customization.cfg").exists()
    }
}
