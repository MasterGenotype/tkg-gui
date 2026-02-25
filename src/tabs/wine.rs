use crate::core::repo_manager::{clone_wine_tkg, CloneMsg};
use crate::core::wine_build_manager::{self, WineBuildHandle, WineBuildMsg};
use crate::core::wine_config_manager;
use crate::settings::AppSettings;
use egui::{Color32, Context, RichText, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

// ── Build state ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum BuildState {
    Idle,
    Running,
    Done(i32),
    Failed,
}

#[derive(Clone, Copy, PartialEq)]
enum LogLevel {
    Normal,
    Stage,
    Warning,
    Error,
    Input,
}

struct LogLine {
    text: String,
    level: LogLevel,
}

// ── Tab state ────────────────────────────────────────────────────────────────

pub struct WineTab {
    // Setup / clone
    path_input: String,
    clone_log: Vec<String>,
    clone_rx: Option<Receiver<CloneMsg>>,
    clone_running: bool,
    clone_status: String,

    // Config
    config: HashMap<String, String>,
    config_loaded: bool,
    config_dirty: bool,
    config_status: String,

    // Build
    build_log: Vec<LogLine>,
    build_state: BuildState,
    build_rx: Option<Receiver<WineBuildMsg>>,
    build_handle: Option<WineBuildHandle>,
    build_auto_scroll: bool,
    build_input: String,
}

impl Default for WineTab {
    fn default() -> Self {
        Self {
            path_input: String::new(),
            clone_log: Vec::new(),
            clone_rx: None,
            clone_running: false,
            clone_status: String::new(),
            config: HashMap::new(),
            config_loaded: false,
            config_dirty: false,
            config_status: String::new(),
            build_log: Vec::new(),
            build_state: BuildState::Idle,
            build_rx: None,
            build_handle: None,
            build_auto_scroll: true,
            build_input: String::new(),
        }
    }
}

impl WineTab {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &Context, settings: &mut AppSettings) {
        // Sync path text field from settings on first render
        if self.path_input.is_empty() {
            self.path_input = settings.wine_tkg_path.to_string_lossy().to_string();
        }

        self.drain_clone_messages(ctx);
        self.drain_build_messages(ctx);

        let is_cloned = settings.is_wine_cloned();
        // Load config lazily once the repo is cloned
        if is_cloned && !self.config_loaded {
            self.reload_config(&settings.wine_tkg_path);
        }

        ui.heading("🍷 Wine Builder");
        ui.add_space(8.0);

        self.show_setup_section(ui, ctx, settings, is_cloned);
        ui.add_space(8.0);
        self.show_config_section(ui, settings, is_cloned);
        ui.add_space(8.0);
        self.show_build_section(ui, ctx, settings, is_cloned);
    }

    // ── Setup section ────────────────────────────────────────────────────────

    fn show_setup_section(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        settings: &mut AppSettings,
        is_cloned: bool,
    ) {
        egui::CollapsingHeader::new("Setup")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    "Path to a local clone of Frogging-Family/wine-tkg-git. \
                     The Config and Build sections read from this location.",
                );
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.path_input)
                            .desired_width(420.0)
                            .hint_text("/home/user/.local/share/tkg-gui/wine-tkg-git"),
                    );
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui.button("Save Path").clicked() {
                        settings.wine_tkg_path = PathBuf::from(&self.path_input);
                        match settings.save() {
                            Ok(()) => {
                                self.config_loaded = false; // force reload from new path
                            }
                            Err(e) => {
                                self.clone_status = format!("Save failed: {}", e);
                            }
                        }
                    }
                });

                ui.add_space(4.0);

                if is_cloned {
                    ui.label(
                        RichText::new(format!(
                            "✓ wine-tkg-git found at {}",
                            settings.wine_tkg_path.display()
                        ))
                        .color(Color32::GREEN),
                    );
                } else {
                    ui.label(
                        RichText::new(format!(
                            "✗ customization.cfg not found at {}/wine-tkg-git/",
                            settings.wine_tkg_path.display()
                        ))
                        .color(Color32::YELLOW),
                    );
                }

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    let can_clone = !self.clone_running && !is_cloned;

                    if ui
                        .add_enabled(can_clone, egui::Button::new("Clone wine-tkg-git"))
                        .on_hover_text(if is_cloned {
                            "Already cloned at the specified path"
                        } else {
                            "git clone --depth=1 https://github.com/Frogging-Family/wine-tkg-git"
                        })
                        .clicked()
                    {
                        self.start_clone(settings.wine_tkg_path.clone(), ctx.clone());
                    }

                    if self.clone_running {
                        ui.spinner();
                    }

                    if !self.clone_status.is_empty() {
                        ui.label(&self.clone_status);
                    }
                });

                if !self.clone_log.is_empty() {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .id_salt("wine_clone_log")
                        .max_height(120.0)
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
    }

    // ── Config section ───────────────────────────────────────────────────────

    fn show_config_section(
        &mut self,
        ui: &mut Ui,
        settings: &AppSettings,
        is_cloned: bool,
    ) {
        egui::CollapsingHeader::new("Configuration")
            .default_open(true)
            .show(ui, |ui| {
                if !is_cloned {
                    ui.label(
                        RichText::new("Clone wine-tkg-git first to edit configuration.")
                            .color(Color32::GRAY),
                    );
                    return;
                }

                // Save / Reload toolbar
                ui.horizontal(|ui| {
                    if ui.button("Save Config").clicked() {
                        self.save_config(&settings.wine_tkg_path);
                    }
                    if ui.button("Reload").clicked() {
                        self.reload_config(&settings.wine_tkg_path);
                    }
                    if self.config_dirty {
                        ui.label(RichText::new("● unsaved changes").color(Color32::YELLOW));
                    }
                    if !self.config_status.is_empty() {
                        ui.label(&self.config_status);
                    }
                });

                ui.add_space(6.0);

                egui::ScrollArea::vertical()
                    .id_salt("wine_config_scroll")
                    .max_height(320.0)
                    .show(ui, |ui| {
                        self.show_wine_source_group(ui);
                        ui.add_space(4.0);
                        self.show_sync_patches_group(ui);
                        ui.add_space(4.0);
                        self.show_compiler_group(ui);
                        ui.add_space(4.0);
                        self.show_modules_group(ui);
                    });
            });
    }

    fn show_wine_source_group(&mut self, ui: &mut Ui) {
        egui::CollapsingHeader::new("Wine Source")
            .default_open(true)
            .show(ui, |ui| {
                self.config_text_edit(ui, "_wine_version", "Version/tag", "e.g. 9.0, 8.21");
                self.config_text_edit(ui, "_wine_commit", "Specific commit", "git SHA (leave empty for latest)");
            });
    }

    fn show_sync_patches_group(&mut self, ui: &mut Ui) {
        egui::CollapsingHeader::new("Sync & Patches")
            .default_open(true)
            .show(ui, |ui| {
                self.config_checkbox(ui, "_use_staging", "Wine-Staging patches");
                self.config_checkbox(ui, "_esync", "Esync (eventfd-based sync)");
                self.config_checkbox(ui, "_fsync", "Fsync (futex-based sync)");
                self.config_checkbox(ui, "_ntsync", "Ntsync (NT synchronization)");
                self.config_checkbox(ui, "_protonify", "Proton patches");
                self.config_checkbox(ui, "_game_drive", "Game Drive support");
            });
    }

    fn show_compiler_group(&mut self, ui: &mut Ui) {
        egui::CollapsingHeader::new("Compiler")
            .default_open(true)
            .show(ui, |ui| {
                // Compiler combobox
                let current = self.config.get("_compiler").cloned().unwrap_or_default();
                let mut selected = current.clone();
                ui.horizontal(|ui| {
                    ui.label("Compiler:");
                    egui::ComboBox::from_id_salt("wine_compiler")
                        .selected_text(if selected.is_empty() { "gcc" } else { &selected })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut selected, String::new(), "gcc (default)");
                            ui.selectable_value(&mut selected, "clang".to_string(), "clang");
                        });
                });
                if selected != current {
                    self.config.insert("_compiler".to_string(), selected);
                    self.config_dirty = true;
                }

                self.config_checkbox(ui, "_O3", "O3 optimisation (-O3)");
                self.config_checkbox(ui, "_lto", "Link-Time Optimisation (LTO)");
            });
    }

    fn show_modules_group(&mut self, ui: &mut Ui) {
        egui::CollapsingHeader::new("Wine Modules")
            .default_open(true)
            .show(ui, |ui| {
                // _no_wow64 is inverted: checked = WoW64 enabled (no_wow64 = false/"")
                let no_wow = self.config
                    .get("_no_wow64")
                    .map(|v| v == "true" || v == "1")
                    .unwrap_or(false);
                let mut wow_enabled = !no_wow;
                if ui.checkbox(&mut wow_enabled, "Enable WoW64 (32-bit in 64-bit process)").changed() {
                    let val = if wow_enabled { "" } else { "true" };
                    self.config.insert("_no_wow64".to_string(), val.to_string());
                    self.config_dirty = true;
                }
            });
    }

    // ── Build section ────────────────────────────────────────────────────────

    fn show_build_section(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        settings: &AppSettings,
        is_cloned: bool,
    ) {
        egui::CollapsingHeader::new("Build")
            .default_open(true)
            .show(ui, |ui| {
                if !is_cloned {
                    ui.label(
                        RichText::new("Clone wine-tkg-git first to run a build.")
                            .color(Color32::GRAY),
                    );
                    return;
                }

                let work_dir = settings.wine_tkg_path.join("wine-tkg-git");

                ui.horizontal(|ui| {
                    let is_running = self.build_state == BuildState::Running;

                    if ui
                        .add_enabled(
                            !is_running,
                            egui::Button::new(
                                RichText::new("▶ Build Wine").color(Color32::GREEN),
                            ),
                        )
                        .clicked()
                    {
                        self.start_build(&settings.wine_tkg_path, ctx.clone());
                    }

                    if ui
                        .add_enabled(
                            is_running,
                            egui::Button::new(RichText::new("■ Stop").color(Color32::RED)),
                        )
                        .on_hover_text("Stop monitoring (process continues in background)")
                        .clicked()
                    {
                        self.build_rx = None;
                        self.build_handle = None;
                        self.build_state = BuildState::Idle;
                        self.build_log.push(LogLine {
                            text: "==> Stopped monitoring".to_string(),
                            level: LogLevel::Warning,
                        });
                    }

                    ui.label(format!("Working dir: {}", work_dir.display()));
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.build_auto_scroll, "Auto-scroll");
                    if ui.button("Clear").clicked() {
                        self.build_log.clear();
                    }

                    let state_text = match self.build_state {
                        BuildState::Idle => "Idle",
                        BuildState::Running => "Running…",
                        BuildState::Done(0) => "✓ Success",
                        BuildState::Done(code) => {
                            ui.label(
                                RichText::new(format!("✗ Failed ({})", code))
                                    .color(Color32::RED),
                            );
                            return;
                        }
                        BuildState::Failed => {
                            ui.label(RichText::new("✗ Failed").color(Color32::RED));
                            return;
                        }
                    };
                    let color = match self.build_state {
                        BuildState::Idle => Color32::GRAY,
                        BuildState::Running => Color32::YELLOW,
                        BuildState::Done(0) => Color32::GREEN,
                        _ => Color32::RED,
                    };
                    ui.label(RichText::new(state_text).color(color));
                });

                ui.add_space(6.0);

                egui::ScrollArea::vertical()
                    .id_salt("wine_build_log")
                    .stick_to_bottom(self.build_auto_scroll)
                    .max_height(ui.available_height() - 60.0)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        for line in &self.build_log {
                            let color = match line.level {
                                LogLevel::Normal => Color32::LIGHT_GRAY,
                                LogLevel::Stage => Color32::GREEN,
                                LogLevel::Warning => Color32::YELLOW,
                                LogLevel::Error => Color32::RED,
                                LogLevel::Input => Color32::LIGHT_BLUE,
                            };
                            let text = RichText::new(&line.text).color(color).monospace();
                            if line.level == LogLevel::Stage {
                                ui.label(text.strong());
                            } else {
                                ui.label(text);
                            }
                        }
                    });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Input:");
                    let response = ui.add_sized(
                        [ui.available_width() - 80.0, 20.0],
                        egui::TextEdit::singleline(&mut self.build_input)
                            .hint_text("Type response and press Enter...")
                            .font(egui::TextStyle::Monospace),
                    );

                    let can_send =
                        self.build_state == BuildState::Running && self.build_handle.is_some();
                    let send_clicked =
                        ui.add_enabled(can_send, egui::Button::new("Send")).clicked();
                    let enter_pressed =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    if can_send && (send_clicked || enter_pressed) && !self.build_input.is_empty() {
                        if let Some(handle) = &self.build_handle {
                            let text = self.build_input.clone();
                            self.build_log.push(LogLine {
                                text: format!(">>> {}", text),
                                level: LogLevel::Input,
                            });
                            if let Err(e) = handle.send_input(&text) {
                                self.build_log.push(LogLine {
                                    text: format!("Error sending input: {}", e),
                                    level: LogLevel::Error,
                                });
                            }
                            self.build_input.clear();
                        }
                    }

                    if enter_pressed {
                        response.request_focus();
                    }
                });

                if self.build_state == BuildState::Running {
                    ctx.request_repaint();
                }
            });
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn config_text_edit(&mut self, ui: &mut Ui, key: &str, label: &str, hint: &str) {
        let val = self.config.entry(key.to_string()).or_default();
        ui.horizontal(|ui| {
            ui.label(format!("{}:", label));
            let resp = ui.add(
                egui::TextEdit::singleline(val)
                    .desired_width(260.0)
                    .hint_text(hint),
            );
            if resp.changed() {
                self.config_dirty = true;
            }
        });
    }

    fn config_checkbox(&mut self, ui: &mut Ui, key: &str, label: &str) {
        let raw = self.config.get(key).cloned().unwrap_or_default();
        let mut checked = raw == "true" || raw == "1";
        if ui.checkbox(&mut checked, label).changed() {
            let val = if checked { "true" } else { "" };
            self.config.insert(key.to_string(), val.to_string());
            self.config_dirty = true;
        }
    }

    fn reload_config(&mut self, wine_tkg_path: &std::path::Path) {
        match wine_config_manager::load(wine_tkg_path) {
            Ok(mgr) => {
                self.config = mgr.get_all_options();
                self.config_loaded = true;
                self.config_dirty = false;
                self.config_status = String::new();
            }
            Err(e) => {
                self.config_status = format!("Load error: {}", e);
            }
        }
    }

    fn save_config(&mut self, wine_tkg_path: &std::path::Path) {
        match wine_config_manager::load(wine_tkg_path) {
            Ok(mut mgr) => {
                for (key, val) in &self.config {
                    mgr.set_option(key, val);
                }
                match mgr.save() {
                    Ok(()) => {
                        self.config_dirty = false;
                        self.config_status = "Saved.".to_string();
                    }
                    Err(e) => {
                        self.config_status = format!("Save error: {}", e);
                    }
                }
            }
            Err(e) => {
                self.config_status = format!("Load error before save: {}", e);
            }
        }
    }

    fn start_clone(&mut self, dest: PathBuf, ctx: Context) {
        self.clone_log.clear();
        self.clone_status = "Cloning…".to_string();
        self.clone_running = true;
        let (tx, rx) = channel();
        self.clone_rx = Some(rx);
        clone_wine_tkg(dest, tx);
        ctx.request_repaint();
    }

    fn start_build(&mut self, wine_tkg_path: &std::path::Path, ctx: Context) {
        self.build_log.clear();
        self.build_state = BuildState::Running;
        self.build_log.push(LogLine {
            text: format!(
                "==> Starting wine build in {}/wine-tkg-git/",
                wine_tkg_path.display()
            ),
            level: LogLevel::Stage,
        });
        self.build_log.push(LogLine {
            text: "==> Running makepkg -si".to_string(),
            level: LogLevel::Stage,
        });
        self.build_log.push(LogLine {
            text: "    (Use the input field below to respond to prompts)".to_string(),
            level: LogLevel::Normal,
        });

        let (tx, rx) = channel();
        self.build_rx = Some(rx);
        let handle = wine_build_manager::start_build(wine_tkg_path.to_path_buf(), tx);
        self.build_handle = Some(handle);
        ctx.request_repaint();
    }

    fn drain_clone_messages(&mut self, ctx: &Context) {
        let mut done = false;
        let mut got = false;
        if let Some(rx) = &self.clone_rx {
            while let Ok(msg) = rx.try_recv() {
                got = true;
                match msg {
                    CloneMsg::Line(line) => self.clone_log.push(line),
                    CloneMsg::Exit(code) => {
                        self.clone_status = if code == 0 {
                            "Clone completed successfully.".to_string()
                        } else {
                            format!("Clone finished with exit code {}.", code)
                        };
                        done = true;
                        // Reset so the config section auto-loads on next frame
                        self.config_loaded = false;
                    }
                    CloneMsg::SpawnError(e) => {
                        self.clone_status = format!("Error: {}", e);
                        done = true;
                    }
                }
            }
        }
        if done {
            self.clone_rx = None;
            self.clone_running = false;
        }
        if got || self.clone_running {
            ctx.request_repaint();
        }
    }

    fn drain_build_messages(&mut self, ctx: &Context) {
        let mut done = false;
        let mut got = false;
        if let Some(rx) = &self.build_rx {
            while let Ok(msg) = rx.try_recv() {
                got = true;
                match msg {
                    WineBuildMsg::Line(text) => {
                        let level = classify_line(&text);
                        self.build_log.push(LogLine { text, level });
                    }
                    WineBuildMsg::Exit(code) => {
                        self.build_state = BuildState::Done(code);
                        self.build_log.push(LogLine {
                            text: format!("==> Build finished with exit code {}", code),
                            level: if code == 0 {
                                LogLevel::Stage
                            } else {
                                LogLevel::Error
                            },
                        });
                        done = true;
                    }
                    WineBuildMsg::SpawnError(e) => {
                        self.build_state = BuildState::Failed;
                        self.build_log.push(LogLine {
                            text: format!("Error: {}", e),
                            level: LogLevel::Error,
                        });
                        done = true;
                    }
                }
            }
        }
        if done {
            self.build_rx = None;
            self.build_handle = None;
        }
        if got {
            ctx.request_repaint();
        }
    }
}

fn classify_line(text: &str) -> LogLevel {
    if text.starts_with("==>") {
        LogLevel::Stage
    } else if text.contains("warning:") || text.contains("WARNING") {
        LogLevel::Warning
    } else if text.contains("error:") || text.contains("ERROR") || text.contains("FAILED") {
        LogLevel::Error
    } else {
        LogLevel::Normal
    }
}
