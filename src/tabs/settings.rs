use crate::core::repo_manager::{clone_linux_tkg, copy_linux_tkg, CloneMsg};
use crate::settings::{default_patch_repo, AppSettings};
use egui::{Color32, Context, RichText, Ui};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

#[derive(Default)]
pub struct SettingsTab {
    // Clone/copy state
    clone_log: Vec<String>,
    clone_rx: Option<Receiver<CloneMsg>>,
    clone_running: bool,
    clone_status: String,

    // Install state
    install_status: String,

    // Patch repository config state
    repo_status: String,
}

impl SettingsTab {
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        settings: &mut AppSettings,
        work_dir_root: &Path,
        linux_tkg_path: &Path,
    ) {
        // Drain clone/copy output
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
                            self.clone_status = "Completed successfully.".to_string();
                        } else {
                            self.clone_status = format!("Finished with exit code {}.", code);
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

        // ── Work Directory ──────────────────────────────────────────────────────
        egui::CollapsingHeader::new("Work Directory")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    "All build operations run inside a temporary directory. \
                     It is cleaned up on exit (or on crash).",
                );
                ui.add_space(4.0);

                ui.label(format!("Work dir: {}", work_dir_root.display()));
                ui.label(format!("linux-tkg: {}", linux_tkg_path.display()));

                ui.add_space(4.0);

                // linux-tkg status in work dir
                let is_ready = linux_tkg_path.join("customization.cfg").exists();
                if is_ready {
                    ui.label(RichText::new("✓ linux-tkg ready").color(Color32::GREEN));
                } else {
                    ui.label(
                        RichText::new("✗ linux-tkg not found in work directory")
                            .color(Color32::YELLOW),
                    );
                }

                ui.add_space(8.0);

                // Clone / copy buttons
                ui.horizontal(|ui| {
                    let can_act = !self.clone_running && !is_ready;

                    // Offer copy from system-installed path if available
                    let installed = AppSettings::installed_linux_tkg_path();
                    let has_installed = installed.join("customization.cfg").exists();

                    // Also check the user's configured reference path
                    let has_settings_ref = settings.is_cloned();

                    if has_installed {
                        if ui
                            .add_enabled(can_act, egui::Button::new("📋 Copy from System"))
                            .on_hover_text(format!("Copy {}", installed.display()))
                            .clicked()
                        {
                            self.start_copy(&installed, linux_tkg_path, ctx.clone());
                        }
                    } else if has_settings_ref
                        && ui
                            .add_enabled(can_act, egui::Button::new("📋 Copy from Local"))
                            .on_hover_text(format!("Copy {}", settings.linux_tkg_path.display()))
                            .clicked()
                    {
                        self.start_copy(&settings.linux_tkg_path, linux_tkg_path, ctx.clone());
                    }

                    if ui
                        .add_enabled(can_act, egui::Button::new("🌐 Clone from GitHub"))
                        .on_hover_text(
                            "git clone --depth=1 https://github.com/Frogging-Family/linux-tkg",
                        )
                        .clicked()
                    {
                        self.start_clone(linux_tkg_path.to_path_buf(), ctx.clone());
                    }

                    if self.clone_running {
                        ui.spinner();
                    }

                    if !self.clone_status.is_empty() {
                        ui.label(&self.clone_status);
                    }
                });

                // Clone/copy log
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

        // ── Install ──────────────────────────────────────────────────────────────
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

        // ── Patch Repository ─────────────────────────────────────────────────────
        egui::CollapsingHeader::new("Patch Repository")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    "GitHub owner/repo browsed by the Patches tab. It must be laid out \
                     like sirlucjan/kernel-patches: a top-level directory per kernel \
                     series, each containing patchset subdirectories.",
                );
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Repo:");
                    ui.add(
                        egui::TextEdit::singleline(&mut settings.patch_repo)
                            .hint_text("owner/repo")
                            .desired_width(260.0),
                    );
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("💾 Save").clicked() {
                        self.repo_status = match settings.save() {
                            Ok(()) => "Saved.".to_string(),
                            Err(e) => format!("Error: {}", e),
                        };
                    }

                    let default_repo = default_patch_repo();
                    if ui
                        .add_enabled(
                            settings.patch_repo != default_repo,
                            egui::Button::new("↩ Reset to Default"),
                        )
                        .on_hover_text(&default_repo)
                        .clicked()
                    {
                        settings.patch_repo = default_repo;
                        self.repo_status = match settings.save() {
                            Ok(()) => "Reset to default.".to_string(),
                            Err(e) => format!("Error: {}", e),
                        };
                    }

                    if !self.repo_status.is_empty() {
                        ui.label(&self.repo_status);
                    }
                });
            });

        ui.add_space(8.0);

        // ── Paths info ───────────────────────────────────────────────────────────
        egui::CollapsingHeader::new("App Directories")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(format!(
                    "Settings:       {}",
                    AppSettings::config_dir().join("settings.json").display()
                ));
                ui.label(format!(
                    "Patch registry: {}",
                    AppSettings::data_dir()
                        .join("patch_registry.json")
                        .display()
                ));
                ui.label(format!("Work directory: {}", work_dir_root.display()));
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

    fn start_copy(&mut self, source: &Path, dest: &Path, ctx: Context) {
        self.clone_log.clear();
        self.clone_status = "Copying…".to_string();
        self.clone_running = true;

        let (tx, rx) = channel();
        self.clone_rx = Some(rx);
        copy_linux_tkg(source, dest, tx);
        ctx.request_repaint();
    }

    fn install_to_local_bin(&mut self) {
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                self.install_status = format!("Could not determine current executable path: {}", e);
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
