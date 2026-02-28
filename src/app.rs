use crate::core::work_dir::WorkDir;
use crate::settings::AppSettings;
use crate::tabs::{
    build::BuildTab, config::ConfigTab, kernel::KernelTab, patches::PatchesTab,
    settings::SettingsTab,
};

#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    Kernel,
    Config,
    Patches,
    Build,
    Settings,
}

pub struct TkgApp {
    active_tab: Tab,
    kernel_tab: KernelTab,
    config_tab: ConfigTab,
    patches_tab: PatchesTab,
    build_tab: BuildTab,
    settings_tab: SettingsTab,
    settings: AppSettings,
    work_dir: WorkDir,
    show_close_dialog: bool,
    close_confirmed: bool,
}

impl TkgApp {
    pub fn new() -> Self {
        let settings = AppSettings::load();
        let work_dir = WorkDir::new().expect("Failed to create temporary work directory");
        Self {
            active_tab: Tab::Kernel,
            kernel_tab: KernelTab::default(),
            config_tab: ConfigTab::default(),
            patches_tab: PatchesTab::default(),
            build_tab: BuildTab::default(),
            settings_tab: SettingsTab::default(),
            settings,
            work_dir,
            show_close_dialog: false,
            close_confirmed: false,
        }
    }
}

impl eframe::App for TkgApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Intercept window close to prompt for cleanup
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.close_confirmed {
                // Allow close — Drop handles cleanup based on keep flag
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_close_dialog = true;
            }
        }

        // Close confirmation dialog
        if self.show_close_dialog {
            egui::Window::new("Exit")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Work directory: {}",
                        self.work_dir.root().display()
                    ));
                    ui.add_space(8.0);
                    ui.label("Clean up temporary build files?");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("🗑 Clean Up & Exit").clicked() {
                            self.work_dir.set_keep(false);
                            self.close_confirmed = true;
                            self.show_close_dialog = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("📁 Keep Files & Exit").clicked() {
                            self.work_dir.set_keep(true);
                            self.close_confirmed = true;
                            self.show_close_dialog = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_close_dialog = false;
                        }
                    });
                });
        }

        // All mutable operations use paths inside the temp work directory
        let linux_tkg_path = self.work_dir.linux_tkg();
        let kernel_sources_dir = self.work_dir.kernel_sources();
        let work_dir_root = self.work_dir.root().to_path_buf();
        let data_dir = AppSettings::data_dir();

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, Tab::Kernel, "🐧 Kernel");
                ui.selectable_value(&mut self.active_tab, Tab::Config, "⚙ Config");
                ui.selectable_value(&mut self.active_tab, Tab::Patches, "🩹 Patches");
                ui.selectable_value(&mut self.active_tab, Tab::Build, "🔨 Build");
                ui.selectable_value(&mut self.active_tab, Tab::Settings, "🔧 Settings");

                ui.separator();

                // Sync button to apply selected kernel version to config
                if let Some(version) = self.kernel_tab.get_selected_version() {
                    if ui.button("📋 Apply Version to Config").clicked() {
                        self.config_tab.set_version(&version);
                        self.config_tab.save_to(&linux_tkg_path);
                        if let Some(series) = self.kernel_tab.get_kernel_series() {
                            self.patches_tab.set_kernel_series(&series);
                        }
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                Tab::Kernel => self.kernel_tab.ui(ui, ctx, &kernel_sources_dir),
                Tab::Config => self.config_tab.ui(ui, &linux_tkg_path),
                Tab::Patches => self.patches_tab.ui(ui, ctx, &linux_tkg_path, &data_dir),
                Tab::Build => self.build_tab.ui(ui, ctx, &linux_tkg_path),
                Tab::Settings => {
                    self.settings_tab
                        .ui(ui, ctx, &mut self.settings, &work_dir_root, &linux_tkg_path);
                }
            }
        });
    }
}
