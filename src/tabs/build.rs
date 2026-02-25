use crate::core::build_manager::{self, BuildHandle, BuildMsg};
use crate::core::config_manager::ConfigManager;
use egui::{Context, RichText, Ui};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};

#[derive(Clone, Copy, PartialEq)]
pub enum BuildState {
    Idle,
    Running,
    Done(i32),
    Failed,
}

#[derive(Clone, Copy, PartialEq)]
pub enum LogLevel {
    Normal,
    Stage,
    Warning,
    Error,
    Input,
}

pub struct LogLine {
    pub text: String,
    pub level: LogLevel,
}

pub struct BuildTab {
    log: Vec<LogLine>,
    state: BuildState,
    rx: Option<Receiver<BuildMsg>>,
    build_handle: Option<BuildHandle>,
    auto_scroll: bool,
    input_text: String,
}

impl Default for BuildTab {
    fn default() -> Self {
        Self {
            log: Vec::new(),
            state: BuildState::Idle,
            rx: None,
            build_handle: None,
            auto_scroll: true,
            input_text: String::new(),
        }
    }
}

impl BuildTab {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &Context, linux_tkg_path: &Path) {
        // Drain messages from build process
        let mut should_clear_rx = false;
        let mut got_messages = false;
        
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                got_messages = true;
                match msg {
                    BuildMsg::Line(text) => {
                        let level = classify_line(&text);
                        self.log.push(LogLine { text, level });
                    }
                    BuildMsg::Exit(code) => {
                        self.state = BuildState::Done(code);
                        self.log.push(LogLine {
                            text: format!("==> Build finished with exit code {}", code),
                            level: if code == 0 {
                                LogLevel::Stage
                            } else {
                                LogLevel::Error
                            },
                        });
                        should_clear_rx = true;
                    }
                    BuildMsg::SpawnError(e) => {
                        self.state = BuildState::Failed;
                        self.log.push(LogLine {
                            text: format!("Error: {}", e),
                            level: LogLevel::Error,
                        });
                        should_clear_rx = true;
                    }
                }
            }
        }
        
        if should_clear_rx {
            self.rx = None;
            self.build_handle = None;
        }
        if got_messages {
            ctx.request_repaint();
        }

        let work_dir = linux_tkg_path.to_path_buf();

        ui.heading("🔨 Build");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let is_running = self.state == BuildState::Running;

            if ui
                .add_enabled(
                    !is_running,
                    egui::Button::new(RichText::new("▶ Build").color(egui::Color32::GREEN)),
                )
                .clicked()
            {
                self.start_build(&work_dir, ctx.clone());
            }

            // Stop button - note: we can't easily kill the process, just stop listening
            if ui
                .add_enabled(
                    is_running,
                    egui::Button::new(RichText::new("■ Stop").color(egui::Color32::RED)),
                )
                .on_hover_text("Stop monitoring (process continues in background)")
                .clicked()
            {
                self.rx = None;
                self.build_handle = None;
                self.state = BuildState::Idle;
                self.log.push(LogLine {
                    text: "==> Stopped monitoring".to_string(),
                    level: LogLevel::Warning,
                });
            }

            ui.label(format!("Working dir: {}", work_dir.display()));
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.auto_scroll, "Auto-scroll");
            if ui.button("Clear").clicked() {
                self.log.clear();
            }

            // State indicator
            let state_text = match self.state {
                BuildState::Idle => "Idle",
                BuildState::Running => "Running…",
                BuildState::Done(0) => "✓ Success",
                BuildState::Done(code) => {
                    ui.label(RichText::new(format!("✗ Failed ({})", code)).color(egui::Color32::RED));
                    return;
                }
                BuildState::Failed => {
                    ui.label(RichText::new("✗ Failed").color(egui::Color32::RED));
                    return;
                }
            };
            let color = match self.state {
                BuildState::Idle => egui::Color32::GRAY,
                BuildState::Running => egui::Color32::YELLOW,
                BuildState::Done(0) => egui::Color32::GREEN,
                _ => egui::Color32::RED,
            };
            ui.label(RichText::new(state_text).color(color));
        });

        ui.add_space(8.0);

        // Log output
        egui::ScrollArea::vertical()
            .stick_to_bottom(self.auto_scroll)
            .max_height(ui.available_height() - 40.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                for line in &self.log {
                    let color = match line.level {
                        LogLevel::Normal => egui::Color32::LIGHT_GRAY,
                        LogLevel::Stage => egui::Color32::GREEN,
                        LogLevel::Warning => egui::Color32::YELLOW,
                        LogLevel::Error => egui::Color32::RED,
                        LogLevel::Input => egui::Color32::LIGHT_BLUE,
                    };
                    let text = RichText::new(&line.text).color(color).monospace();
                    if line.level == LogLevel::Stage {
                        ui.label(text.strong());
                    } else {
                        ui.label(text);
                    }
                }
            });

        // Input field for interactive builds
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Input:");
            let response = ui.add_sized(
                [ui.available_width() - 80.0, 20.0],
                egui::TextEdit::singleline(&mut self.input_text)
                    .hint_text("Type response and press Enter...")
                    .font(egui::TextStyle::Monospace),
            );

            let can_send = self.state == BuildState::Running && self.build_handle.is_some();
            let send_clicked = ui.add_enabled(can_send, egui::Button::new("Send")).clicked();
            let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if can_send && (send_clicked || enter_pressed) && !self.input_text.is_empty() {
                if let Some(handle) = &self.build_handle {
                    let input = self.input_text.clone();
                    self.log.push(LogLine {
                        text: format!(">>> {}", input),
                        level: LogLevel::Input,
                    });
                    if let Err(e) = handle.send_input(&input) {
                        self.log.push(LogLine {
                            text: format!("Error sending input: {}", e),
                            level: LogLevel::Error,
                        });
                    }
                    self.input_text.clear();
                }
            }

            // Re-focus input field after sending
            if enter_pressed {
                response.request_focus();
            }
        });

        // Keep repainting while building
        if self.state == BuildState::Running {
            ctx.request_repaint();
        }
    }

    fn start_build(&mut self, work_dir: &Path, ctx: Context) {
        self.log.clear();
        self.state = BuildState::Running;
        self.log.push(LogLine {
            text: format!("==> Starting build in {}", work_dir.display()),
            level: LogLevel::Stage,
        });

        // Detect distro from config to determine build command
        let config_path = work_dir.join("customization.cfg");
        let use_makepkg = if let Ok(config) = ConfigManager::load(&config_path) {
            config.get_option("_distro").unwrap_or_default() == "Arch"
        } else {
            false
        };

        let cmd_name = if use_makepkg {
            "makepkg -si"
        } else {
            "./install.sh install"
        };

        self.log.push(LogLine {
            text: format!("==> Running {}", cmd_name),
            level: LogLevel::Stage,
        });
        self.log.push(LogLine {
            text: "    (Use the input field below to respond to prompts)".to_string(),
            level: LogLevel::Normal,
        });

        let (tx, rx) = channel();
        self.rx = Some(rx);

        let handle = build_manager::start_build(work_dir.to_path_buf(), tx, use_makepkg);
        self.build_handle = Some(handle);
        ctx.request_repaint();
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
