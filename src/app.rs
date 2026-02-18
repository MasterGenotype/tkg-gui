use crate::tabs::{build::BuildTab, config::ConfigTab, kernel::KernelTab, patches::PatchesTab};
use std::path::PathBuf;

#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    Kernel,
    Config,
    Patches,
    Build,
}

pub struct TkgApp {
    active_tab: Tab,
    kernel_tab: KernelTab,
    config_tab: ConfigTab,
    patches_tab: PatchesTab,
    build_tab: BuildTab,
    base_dir: PathBuf,
}

impl TkgApp {
    pub fn new() -> Self {
        // Find the base directory (containing submodules/linux-tkg)
        let base_dir = find_base_dir().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        });

        Self {
            active_tab: Tab::Kernel,
            kernel_tab: KernelTab::default(),
            config_tab: ConfigTab::default(),
            patches_tab: PatchesTab::default(),
            build_tab: BuildTab::default(),
            base_dir,
        }
    }
}

impl eframe::App for TkgApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, Tab::Kernel, "🐧 Kernel");
                ui.selectable_value(&mut self.active_tab, Tab::Config, "⚙ Config");
                ui.selectable_value(&mut self.active_tab, Tab::Patches, "🩹 Patches");
                ui.selectable_value(&mut self.active_tab, Tab::Build, "🔨 Build");

                ui.separator();

                // Sync button to apply selected kernel version to config
                if let Some(version) = self.kernel_tab.get_selected_version() {
                    if ui.button("📋 Apply Version to Config").clicked() {
                        self.config_tab.set_version(&version);
                        if let Some(series) = self.kernel_tab.get_kernel_series() {
                            self.patches_tab.set_kernel_series(&series);
                        }
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                Tab::Kernel => self.kernel_tab.ui(ui, ctx),
                Tab::Config => self.config_tab.ui(ui, &self.base_dir),
                Tab::Patches => self.patches_tab.ui(ui, ctx, &self.base_dir),
                Tab::Build => self.build_tab.ui(ui, ctx, &self.base_dir),
            }
        });
    }
}

/// Find the base directory containing submodules/linux-tkg
fn find_base_dir() -> Option<PathBuf> {
    // Try current exe location first
    if let Ok(exe_path) = std::env::current_exe() {
        let mut dir = exe_path.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            let config_path = d.join("submodules/linux-tkg/customization.cfg");
            if config_path.exists() {
                return Some(d);
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    // Try current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = Some(cwd);
        while let Some(d) = dir {
            let config_path = d.join("submodules/linux-tkg/customization.cfg");
            if config_path.exists() {
                return Some(d);
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    None
}
