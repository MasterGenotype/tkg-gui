use crate::core::repo_manager::{clone_linux_tkg, CloneMsg};
use crate::settings::AppSettings;
use egui::{Color32, Context, RichText, Ui};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

pub struct SettingsTab {
    /// Editable copy of the linux-tkg path (String for the text field)
    path_input: String,
    save_status: String,

    // Clone state
    clone_log: Vec<String>,
    clone_rx: Option<Receiver<CloneMsg>>,
    clone_running: bool,
    clone_status: String,

    // Install state
    install_status: String,
}

impl Default for SettingsTab {
    fn default() -> Self {
        Self {
            path_input: String::new(),
            save_status: String::new(),
            clone_log: Vec::new(),
            clone_rx: None,
            clone_running: false,
            clone_status: String::new(),
            install_status: String::new(),
        }
    }
}

impl SettingsTab {
    /// Called once when the tab first becomes active; syncs the text field
    /// from the current settings.
    pub fn sync_from_settings(&mut self, settings: &AppSettings) {
        self.path_input = settings.linux_tkg_path.to_string_lossy().to_string();
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &Context, settings: &mut AppSettings) {
        // Initialise text field on first use
        if self.path_input.is_empty() {
            self.path_input = settings.linux_tkg_path.to_string_lossy().to_string();
        }

        // Drain clone output
        let mut clone_done = false;
        if let Some(rx) = &self.clone_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    CloneMsg::Line(line) => {
                        self.clone_log.push(line);
                        ctx.request_repaint();
                    }
                    CloneMsg::Exit(code) => {
                        if code == 0 {
                            self.clone_status = "Clone completed successfully.".to_string();
                        } else {
                            self.clone_status =
                                format!("Clone finished with exit code {}.", code);
                        }
                        clone_done = true;
                        ctx.request_repaint();
                    }
                    CloneMsg::SpawnError(e) => {
                        self.clone_status = format!("Error: {}", e);
                        clone_done = true;
                        ctx.request_repaint();
                    }
                }
            }
        }
        if clone_done {
            self.clone_rx = None;
            self.clone_running = false;
        }
        if self.clone_running {
            ctx.request_repaint();
        }

        ui.heading("Settings");
        ui.add_space(8.0);

        // ── linux-tkg Path ───────────────────────────────────────────────
        egui::CollapsingHeader::new("linux-tkg Repository Path")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    "Path to a local clone of Frogging-Family/linux-tkg. \
                     The Config, Patches and Build tabs all read from this location.",
                );
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.path_input)
                            .desired_width(420.0)
                            .hint_text("/home/user/.local/share/tkg-gui/linux-tkg"),
                    );
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui.button("Save Path").clicked() {
                        let new_path = PathBuf::from(&self.path_input);
                        settings.linux_tkg_path = new_path;
                        match settings.save() {
                            Ok(()) => {
                                self.save_status =
                                    format!("Saved. Path: {}", settings.linux_tkg_path.display());
                            }
                            Err(e) => {
                                self.save_status = format!("Save failed: {}", e);
                            }
                        }
                    }

                    if !self.save_status.is_empty() {
                        ui.label(RichText::new(&self.save_status).color(Color32::YELLOW));
                    }
                });

                ui.add_space(4.0);

                // Status indicator: is linux-tkg already cloned here?
                let is_cloned = settings.is_cloned();
                if is_cloned {
                    ui.label(
                        RichText::new(format!(
                            "✓ linux-tkg found at {}",
                            settings.linux_tkg_path.display()
                        ))
                        .color(Color32::GREEN),
                    );
                } else {
                    ui.label(
                        RichText::new(format!(
                            "✗ customization.cfg not found at {}",
                            settings.linux_tkg_path.display()
                        ))
                        .color(Color32::YELLOW),
                    );
                }

                ui.add_space(8.0);

                // Clone button
                ui.horizontal(|ui| {
                    let can_clone = !self.clone_running && !is_cloned;

                    if ui
                        .add_enabled(can_clone, egui::Button::new("Clone linux-tkg"))
                        .on_hover_text(if is_cloned {
                            "Already cloned at the specified path"
                        } else {
                            "git clone --depth=1 https://github.com/Frogging-Family/linux-tkg"
                        })
                        .clicked()
                    {
                        self.start_clone(settings.linux_tkg_path.clone(), ctx.clone());
                    }

                    if self.clone_running {
                        ui.spinner();
                    }

                    if !self.clone_status.is_empty() {
                        ui.label(&self.clone_status);
                    }
                });

                // Clone log
                if !self.clone_log.is_empty() {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .id_salt("clone_log")
                        .max_height(160.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.clone_log {
                                ui.label(
                                    RichText::new(line)
                                        .monospace()
                                        .small()
                                        .color(Color32::LIGHT_GRAY),
                                );
                            }
                        });
                }
            });

        ui.add_space(8.0);

        // ── Install ──────────────────────────────────────────────────────
        egui::CollapsingHeader::new("Install tkg-gui")
            .default_open(true)
            .show(ui, |ui| {
                let local_bin = home_local_bin();
                ui.label(format!(
                    "Install the running tkg-gui binary to {}",
                    local_bin.display()
                ));
                ui.add_space(4.0);

                if ui.button("Install to ~/.local/bin").clicked() {
                    self.install_to_local_bin();
                }

                if !self.install_status.is_empty() {
                    ui.add_space(4.0);
                    let color = if self.install_status.starts_with("Installed") {
                        Color32::GREEN
                    } else {
                        Color32::RED
                    };
                    ui.label(RichText::new(&self.install_status).color(color));
                }
            });

        ui.add_space(8.0);

        // ── Paths info ───────────────────────────────────────────────────
        egui::CollapsingHeader::new("App Directories")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(format!(
                    "Settings:       {}",
                    AppSettings::config_dir().join("settings.json").display()
                ));
                ui.label(format!(
                    "Patch registry: {}",
                    AppSettings::data_dir().join("patch_registry.json").display()
                ));
                ui.label(format!(
                    "linux-tkg:      {}",
                    settings.linux_tkg_path.display()
                ));
                ui.label(format!(
                    "wine-tkg-git:   {}",
                    settings.wine_tkg_path.display()
                ));
            });
    }

    fn start_clone(&mut self, dest: PathBuf, ctx: Context) {
        self.clone_log.clear();
        self.clone_status = "Cloning…".to_string();
        self.clone_running = true;

        let (tx, rx) = channel();
        self.clone_rx = Some(rx);
        clone_linux_tkg(dest, tx);
        ctx.request_repaint();
    }

    fn install_to_local_bin(&mut self) {
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                self.install_status =
                    format!("Could not determine current executable path: {}", e);
                return;
            }
        };

        let local_bin = home_local_bin();
        if let Err(e) = std::fs::create_dir_all(&local_bin) {
            self.install_status = format!("Failed to create {}: {}", local_bin.display(), e);
            return;
        }

        let dest = local_bin.join("tkg-gui");
        match std::fs::copy(&exe, &dest) {
            Ok(_) => {
                // Make the binary executable (Unix only)
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(&dest) {
                        let mut perms = meta.permissions();
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(&dest, perms);
                    }
                }
                self.install_status = format!("Installed to {}", dest.display());
            }
            Err(e) => {
                self.install_status = format!("Install failed: {}", e);
            }
        }
    }
}

fn home_local_bin() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local").join("bin")
    } else {
        PathBuf::from(".local").join("bin")
    }
}
