use crate::core::config_manager::ConfigManager;
use egui::Ui;
use std::collections::HashMap;
use std::path::Path;

pub struct ConfigTab {
    values: HashMap<String, String>,
    loaded: bool,
    dirty: bool,
    status: String,
    config_path: Option<std::path::PathBuf>,
}

impl Default for ConfigTab {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            loaded: false,
            dirty: false,
            status: String::new(),
            config_path: None,
        }
    }
}

impl ConfigTab {
    pub fn ui(&mut self, ui: &mut Ui, linux_tkg_path: &Path) {
        let config_path = linux_tkg_path.join("customization.cfg");

        // Reload if the path changed (e.g. user updated settings)
        if self.config_path.as_deref() != Some(config_path.as_path()) {
            self.loaded = false;
        }

        // Load config if not loaded
        if !self.loaded {
            self.load_config(&config_path);
        }

        ui.heading("⚙ Configuration Options");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.button("💾 Save Config").clicked() {
                self.save_config(&config_path);
            }
            if ui.button("🔄 Reload").clicked() {
                self.load_config(&config_path);
            }
            if self.dirty {
                ui.label(egui::RichText::new("● Modified").color(egui::Color32::YELLOW));
            }
            ui.label(&self.status);
        });

        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            // CPU Scheduling
            egui::CollapsingHeader::new("CPU Scheduling")
                .default_open(true)
                .show(ui, |ui| {
                    self.combo_option(ui, "_cpusched", "CPU Scheduler", &[
                        ("", "Default"),
                        ("pds", "PDS"),
                        ("bmq", "BMQ"),
                        ("bore", "BORE"),
                        ("cfs", "CFS"),
                        ("eevdf", "EEVDF"),
                        ("upds", "UPDS"),
                        ("muqss", "MuQSS"),
                    ]);
                });

            // Compiler
            egui::CollapsingHeader::new("Compiler")
                .default_open(true)
                .show(ui, |ui| {
                    self.combo_option(ui, "_compiler", "Compiler", &[
                        ("", "GCC"),
                        ("llvm", "LLVM/Clang"),
                    ]);
                    self.combo_option(ui, "_compileroptlevel", "Optimization Level", &[
                        ("1", "-O2"),
                        ("2", "-O3"),
                        ("3", "-Os"),
                    ]);
                    self.combo_option(ui, "_lto_mode", "LTO Mode", &[
                        ("", "Default"),
                        ("no", "Disabled"),
                        ("full", "Full LTO"),
                        ("thin", "Thin LTO"),
                    ]);
                    self.combo_option(ui, "_llvm_ias", "LLVM Integrated Assembler", &[
                        ("0", "Disabled"),
                        ("1", "Enabled"),
                    ]);
                });

            // Kernel Version & Source
            egui::CollapsingHeader::new("Kernel Version & Source")
                .default_open(true)
                .show(ui, |ui| {
                    self.text_option(ui, "_version", "Kernel Version");
                    self.combo_option(ui, "_git_mirror", "Git Mirror", &[
                        ("kernel.org", "kernel.org"),
                        ("googlesource.com", "googlesource.com"),
                        ("gregkh", "gregkh"),
                        ("torvalds", "torvalds"),
                    ]);
                    self.combo_option(ui, "_distro", "Distribution", &[
                        ("Arch", "Arch"),
                        ("Ubuntu", "Ubuntu"),
                        ("Debian", "Debian"),
                        ("Fedora", "Fedora"),
                        ("Suse", "Suse"),
                        ("Gentoo", "Gentoo"),
                        ("Generic", "Generic"),
                    ]);
                });

            // CPU & Performance
            egui::CollapsingHeader::new("CPU & Performance")
                .default_open(false)
                .show(ui, |ui| {
                    self.combo_option(ui, "_processor_opt", "Processor Optimization", &[
                        ("", "Default"),
                        ("generic", "Generic"),
                        ("zen", "Zen"),
                        ("zen2", "Zen 2"),
                        ("zen3", "Zen 3"),
                        ("zen4", "Zen 4"),
                        ("skylake", "Skylake"),
                        ("native_amd", "Native AMD"),
                        ("native_intel", "Native Intel"),
                        ("intel", "Intel"),
                    ]);
                    self.combo_option(ui, "_timer_freq", "Timer Frequency", &[
                        ("100", "100 Hz"),
                        ("250", "250 Hz"),
                        ("300", "300 Hz"),
                        ("500", "500 Hz"),
                        ("750", "750 Hz"),
                        ("1000", "1000 Hz"),
                    ]);
                    self.combo_option(ui, "_tickless", "Tickless Mode", &[
                        ("0", "Periodic"),
                        ("1", "Full"),
                        ("2", "Idle"),
                    ]);
                    self.combo_option(ui, "_tcp_cong_alg", "TCP Congestion Algorithm", &[
                        ("", "Default"),
                        ("yeah", "YeAH"),
                        ("bbr", "BBR"),
                        ("cubic", "CUBIC"),
                        ("reno", "Reno"),
                        ("vegas", "Vegas"),
                        ("westwood", "Westwood"),
                    ]);
                    self.combo_option(ui, "_default_cpu_gov", "Default CPU Governor", &[
                        ("", "Default"),
                        ("performance", "Performance"),
                        ("ondemand", "Ondemand"),
                        ("schedutil", "Schedutil"),
                    ]);
                    self.combo_option(ui, "_rqshare", "RQ Share", &[
                        ("none", "None"),
                        ("smt", "SMT"),
                        ("mc", "MC"),
                        ("mc-llc", "MC-LLC"),
                        ("smp", "SMP"),
                        ("all", "All"),
                    ]);
                });

            // Configuration Management
            egui::CollapsingHeader::new("Configuration Management")
                .default_open(false)
                .show(ui, |ui| {
                    self.text_option(ui, "_configfile", "Config File Path");
                    self.combo_option(ui, "_config_updating", "Config Updating", &[
                        ("olddefconfig", "olddefconfig"),
                        ("oldconfig", "oldconfig"),
                    ]);
                    self.combo_option(ui, "_menunconfig", "Menu Config", &[
                        ("false", "Disabled"),
                        ("1", "menuconfig"),
                        ("2", "nconfig"),
                        ("3", "xconfig"),
                    ]);
                });

            // Patches & Features
            egui::CollapsingHeader::new("Patches & Features")
                .default_open(false)
                .show(ui, |ui| {
                    self.checkbox_option(ui, "_user_patches", "User Patches");
                    self.text_option(ui, "_community_patches", "Community Patches");
                    self.checkbox_option(ui, "_clear_patches", "Clear Linux Patches");
                    self.checkbox_option(ui, "_openrgb", "OpenRGB");
                    self.checkbox_option(ui, "_acs_override", "ACS Override");
                    self.checkbox_option(ui, "_preempt_rt", "PREEMPT_RT");
                    self.checkbox_option(ui, "_fsync_backport", "Fsync Backport");
                    self.checkbox_option(ui, "_ntsync", "NTSync");
                    self.checkbox_option(ui, "_zenify", "Zenify");
                    self.checkbox_option(ui, "_glitched_base", "Glitched Base");
                    self.checkbox_option(ui, "_bcachefs", "Bcachefs");
                });

            // Build & Debug
            egui::CollapsingHeader::new("Build & Debug")
                .default_open(false)
                .show(ui, |ui| {
                    self.checkbox_option(ui, "_debugdisable", "Disable Debug");
                    self.checkbox_option(ui, "_STRIP", "Strip Binaries");
                    self.checkbox_option(ui, "_ftracedisable", "Disable Ftrace");
                    self.checkbox_option(ui, "_numadisable", "Disable NUMA");
                    self.checkbox_option(ui, "_misc_adds", "Misc Additions");
                    self.checkbox_option(ui, "_kernel_on_diet", "Kernel on Diet");
                    self.checkbox_option(ui, "_modprobeddb", "modprobed-db");
                    self.checkbox_option(ui, "_random_trust_cpu", "Trust CPU RNG");
                    self.checkbox_option(ui, "_config_fragments", "Config Fragments");
                    self.checkbox_option(ui, "_NUKR", "NUKR");
                    self.checkbox_option(ui, "_force_all_threads", "Force All Threads");
                });
        });
    }

    fn load_config(&mut self, path: &Path) {
        match ConfigManager::load(path) {
            Ok(manager) => {
                self.values = manager.get_all_options();
                self.loaded = true;
                self.dirty = false;
                self.config_path = Some(path.to_path_buf());
                self.status = "Config loaded".to_string();
            }
            Err(e) => {
                self.status = format!("Error loading config: {}", e);
            }
        }
    }

    fn save_config(&mut self, path: &Path) {
        match ConfigManager::load(path) {
            Ok(mut manager) => {
                for (key, value) in &self.values {
                    manager.set_option(key, value);
                }
                match manager.save() {
                    Ok(()) => {
                        self.dirty = false;
                        self.status = "Config saved".to_string();
                    }
                    Err(e) => {
                        self.status = format!("Error saving: {}", e);
                    }
                }
            }
            Err(e) => {
                self.status = format!("Error: {}", e);
            }
        }
    }

    fn combo_option(&mut self, ui: &mut Ui, key: &str, label: &str, options: &[(&str, &str)]) {
        let current = self.values.get(key).cloned().unwrap_or_default();
        let current_label = options
            .iter()
            .find(|(v, _)| *v == current)
            .map(|(_, l)| *l)
            .unwrap_or(&current);

        ui.horizontal(|ui| {
            ui.label(format!("{}:", label));
            egui::ComboBox::from_id_salt(key)
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    for (value, label) in options {
                        if ui.selectable_label(current == *value, *label).clicked() {
                            self.values.insert(key.to_string(), value.to_string());
                            self.dirty = true;
                        }
                    }
                });
        });
    }

    fn text_option(&mut self, ui: &mut Ui, key: &str, label: &str) {
        let mut value = self.values.get(key).cloned().unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(format!("{}:", label));
            if ui.text_edit_singleline(&mut value).changed() {
                self.values.insert(key.to_string(), value);
                self.dirty = true;
            }
        });
    }

    fn checkbox_option(&mut self, ui: &mut Ui, key: &str, label: &str) {
        let value = self.values.get(key).cloned().unwrap_or_default();
        let mut checked = value == "true" || value == "1";
        if ui.checkbox(&mut checked, label).changed() {
            self.values.insert(key.to_string(), if checked { "true" } else { "false" }.to_string());
            self.dirty = true;
        }
    }

    pub fn set_version(&mut self, version: &str) {
        self.values.insert("_version".to_string(), version.to_string());
        self.dirty = true;
    }

    pub fn get_version(&self) -> Option<String> {
        self.values.get("_version").cloned()
    }
}
