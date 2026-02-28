use crate::core::config_manager::ConfigManager;
use egui::Ui;
use std::collections::HashMap;
use std::path::Path;

#[derive(Default)]
pub struct ConfigTab {
    values: HashMap<String, String>,
    loaded: bool,
    dirty: bool,
    status: String,
    config_path: Option<std::path::PathBuf>,
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
                    self.combo_option(ui, "_sched_yield_type", "Sched Yield Type", &[
                        ("0", "No yield"),
                        ("1", "Yield to better priority (default)"),
                        ("2", "Expire timeslice"),
                    ]);
                    self.combo_option(ui, "_rr_interval", "Round Robin Interval", &[
                        ("default", "Default"),
                        ("2", "2ms"),
                        ("4", "4ms"),
                        ("6", "6ms"),
                        ("8", "8ms"),
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
                        ("x86-64", "x86-64 (baseline)"),
                        ("x86-64-v2", "x86-64-v2 (~2008+)"),
                        ("x86-64-v3", "x86-64-v3 (~2013+)"),
                        ("x86-64-v4", "x86-64-v4 (Skylake/Zen4+)"),
                        ("native", "Native (auto-detect)"),
                        ("znver5", "Zen 5 (Ryzen 9000)"),
                        ("znver4", "Zen 4 (Ryzen 7000/8000)"),
                        ("znver3", "Zen 3 (Ryzen 5000/6000)"),
                        ("znver2", "Zen 2 (Ryzen 3000/4000)"),
                        ("znver1", "Zen 1 (Ryzen 1000/2000)"),
                        ("arrowlake-s", "Arrow Lake-S (Core Ultra 200)"),
                        ("raptorlake", "Raptor Lake (13th/14th gen)"),
                        ("alderlake", "Alder Lake (12th gen)"),
                        ("skylake", "Skylake (6th-9th gen)"),
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
                    self.text_option(ui, "_kernel_work_folder", "Kernel Work Folder");
                    self.text_option(ui, "_kernel_source_folder", "Kernel Source Folder");
                    self.checkbox_option(ui, "_offline", "Offline Mode");
                    self.checkbox_option(ui, "_nofallback", "No Fallback (exit on error)");
                });

            // Patches & Features
            egui::CollapsingHeader::new("Patches & Features")
                .default_open(false)
                .show(ui, |ui| {
                    self.checkbox_option(ui, "_user_patches", "User Patches");
                    self.checkbox_option(ui, "_user_patches_no_confirm", "Skip User Patch Confirm");
                    self.text_option(ui, "_community_patches", "Community Patches");
                    self.checkbox_option(ui, "_clear_patches", "Clear Linux Patches");
                    self.checkbox_option(ui, "_openrgb", "OpenRGB");
                    self.checkbox_option(ui, "_acs_override", "ACS Override");
                    self.checkbox_option(ui, "_preempt_rt", "PREEMPT_RT");
                    self.checkbox_option(ui, "_fsync_backport", "Fsync Backport");
                    self.checkbox_option(ui, "_fsync_legacy", "Fsync Legacy");
                    self.checkbox_option(ui, "_ntsync", "NTSync");
                    self.checkbox_option(ui, "_zenify", "Zenify");
                    self.checkbox_option(ui, "_glitched_base", "Glitched Base");
                    self.checkbox_option(ui, "_mglru", "MGLRU (Multi-Gen LRU)");
                    self.checkbox_option(ui, "_irq_threading", "Force IRQ Threading");
                    self.checkbox_option(ui, "_smt_nice", "SMT Nice");
                    self.checkbox_option(ui, "_random_trust_cpu", "Trust CPU RNG");
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
                    self.text_option(ui, "_modprobeddb_db_path", "modprobed-db Path");
                    self.checkbox_option(ui, "_config_fragments", "Config Fragments");
                    self.checkbox_option(ui, "_config_fragments_no_confirm", "Skip Config Fragments Confirm");
                    self.checkbox_option(ui, "_NUKR", "NUKR");
                    self.checkbox_option(ui, "_force_all_threads", "Force All Threads");
                    self.combo_option(ui, "_menunconfig", "Menu Config", &[
                        ("0", "Disabled"),
                        ("1", "menuconfig"),
                        ("2", "nconfig"),
                        ("3", "xconfig"),
                    ]);
                    self.combo_option(ui, "_install_after_building", "Install After Building", &[
                        ("prompt", "Prompt"),
                        ("true", "Yes"),
                        ("false", "No"),
                    ]);
                    self.text_option(ui, "_NR_CPUS_value", "Max CPUs (NR_CPUS)");
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
        // Ensure version has 'v' prefix as required by linux-tkg
        let version = if version.starts_with('v') {
            version.to_string()
        } else {
            format!("v{}", version)
        };
        self.values.insert("_version".to_string(), version);
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub fn get_version(&self) -> Option<String> {
        self.values.get("_version").cloned()
    }

    /// Save config to the given linux-tkg directory path
    pub fn save_to(&mut self, linux_tkg_path: &std::path::Path) {
        let config_path = linux_tkg_path.join("customization.cfg");
        self.save_config(&config_path);
    }

}
