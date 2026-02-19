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
}

impl TkgApp {
    pub fn new() -> Self {
        let settings = AppSettings::load();
        Self {
            active_tab: Tab::Kernel,
            kernel_tab: KernelTab::default(),
            config_tab: ConfigTab::default(),
            patches_tab: PatchesTab::default(),
            build_tab: BuildTab::default(),
            settings_tab: SettingsTab::default(),
            settings,
        }
    }
}

impl eframe::App for TkgApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let linux_tkg_path = self.settings.linux_tkg_path.clone();
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
                Tab::Kernel => self.kernel_tab.ui(ui, ctx),
                Tab::Config => self.config_tab.ui(ui, &linux_tkg_path),
                Tab::Patches => self.patches_tab.ui(ui, ctx, &linux_tkg_path, &data_dir),
                Tab::Build => self.build_tab.ui(ui, ctx, &linux_tkg_path),
                Tab::Settings => {
                    self.settings_tab.ui(ui, ctx, &mut self.settings);
                }
            }
        });
    }
}
