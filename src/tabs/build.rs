use crate::core::build_manager::{self, BuildMsg};
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
}

pub struct LogLine {
    pub text: String,
    pub level: LogLevel,
}

pub struct BuildTab {
    log: Vec<LogLine>,
    state: BuildState,
    rx: Option<Receiver<BuildMsg>>,
    auto_scroll: bool,
}

impl Default for BuildTab {
    fn default() -> Self {
        Self {
            log: Vec::new(),
            state: BuildState::Idle,
            rx: None,
            auto_scroll: true,
        }
    }
}

impl BuildTab {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &Context, base_dir: &Path) {
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
        }
        if got_messages {
            ctx.request_repaint();
        }

        let work_dir = base_dir.join("submodules").join("linux-tkg");

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
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                for line in &self.log {
                    let color = match line.level {
                        LogLevel::Normal => egui::Color32::LIGHT_GRAY,
                        LogLevel::Stage => egui::Color32::GREEN,
                        LogLevel::Warning => egui::Color32::YELLOW,
                        LogLevel::Error => egui::Color32::RED,
                    };
                    let text = RichText::new(&line.text).color(color).monospace();
                    if line.level == LogLevel::Stage {
                        ui.label(text.strong());
                    } else {
                        ui.label(text);
                    }
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

        let (tx, rx) = channel();
        self.rx = Some(rx);

        build_manager::start_build(work_dir.to_path_buf(), tx);
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
