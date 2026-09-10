use crate::core::config_manager::ConfigManager;
use crate::core::fragment_manager::{self, FeaturePresets};
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
    fragment_status: String,
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
                self.save_config(&config_path, linux_tkg_path);
            }
            if ui.button("🔄 Reload").clicked() {
                self.load_config(&config_path);
            }
            if ui
                .button("🧩 Sync Fragments")
                .on_hover_text("Write/remove .myfrag files for Xen / LVM thin / acpi_call presets")
                .clicked()
            {
                self.sync_feature_fragments(linux_tkg_path);
            }
            if self.dirty {
                ui.label(egui::RichText::new("● Modified").color(egui::Color32::YELLOW));
            }
            ui.label(&self.status);
        });
        if !self.fragment_status.is_empty() {
            ui.label(egui::RichText::new(&self.fragment_status).color(egui::Color32::LIGHT_BLUE));
        }

        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            // ---- Automated kernel feature presets (myfrag) ----
            egui::CollapsingHeader::new("Automated Kernel Features (.myfrag)")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(
                        "These write linux-tkg config fragments next to PKGBUILD and force \
                         _config_fragments / _config_fragments_no_confirm when enabled.",
                    );
                    ui.add_space(4.0);
                    self.checkbox_option(
                        ui,
                        "_tkg_gui_xen_dom0",
                        "Xen dom0 / backend kernel options",
                    );
                    ui.label(
                        egui::RichText::new(
                            "  Forces CONFIG_XEN* backend/frontend, privcmd, balloon, grant, PCI stub…",
                        )
                        .small()
                        .weak(),
                    );
                    self.checkbox_option(
                        ui,
                        "_tkg_gui_lvm_thin",
                        "LVM thin-provisioning (device-mapper)",
                    );
                    ui.label(
                        egui::RichText::new(
                            "  Forces CONFIG_DM_THIN_PROVISIONING and related DM options",
                        )
                        .small()
                        .weak(),
                    );
                    self.checkbox_option(
                        ui,
                        "_tkg_gui_acpi_call",
                        "acpi_call out-of-tree module support",
                    );
                    ui.label(
                        egui::RichText::new(
                            "  ACPI prereqs + tkg-gui-acpi-call-install.sh (DKMS or source build)",
                        )
                        .small()
                        .weak(),
                    );
                });

            // CPU Scheduling
            egui::CollapsingHeader::new("CPU Scheduling")
                .default_open(true)
                .show(ui, |ui| {
                    self.combo_option(
                        ui,
                        "_cpusched",
                        "CPU Scheduler",
                        &[
                            ("", "Default (prompt)"),
                            ("pds", "PDS"),
                            ("bmq", "BMQ"),
                            ("bore", "BORE"),
                            ("cfs", "CFS"),
                            ("eevdf", "EEVDF"),
                            ("upds", "UPDS"),
                            ("muqss", "MuQSS"),
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_sched_yield_type",
                        "Sched Yield Type",
                        &[
                            ("", "Default (prompt)"),
                            ("0", "No yield"),
                            ("1", "Yield to better priority (default)"),
                            ("2", "Expire timeslice"),
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_rr_interval",
                        "Round Robin Interval",
                        &[
                            ("", "Default (prompt)"),
                            ("default", "Scheduler default"),
                            ("1", "2ms"),
                            ("2", "4ms"),
                            ("3", "6ms"),
                            ("4", "8ms"),
                        ],
                    );
                    self.text_option(ui, "_bore_min_base_slice_ns", "BORE Min Base Slice (ns)");
                    self.combo_option(
                        ui,
                        "_runqueue_sharing",
                        "Runqueue Sharing (MuQSS)",
                        &[
                            ("", "Default"),
                            ("none", "None"),
                            ("smt", "SMT"),
                            ("mc", "MC"),
                            ("mc-llc", "MC-LLC (Zen)"),
                            ("smp", "SMP"),
                            ("all", "All / NUMA"),
                        ],
                    );
                });

            // Compiler
            egui::CollapsingHeader::new("Compiler")
                .default_open(true)
                .show(ui, |ui| {
                    self.combo_option(
                        ui,
                        "_compiler",
                        "Compiler",
                        &[
                            ("", "Default (prompt)"),
                            ("gcc", "GCC"),
                            ("llvm", "LLVM/Clang"),
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_compileroptlevel",
                        "Optimization Level",
                        &[("1", "-O2"), ("2", "-O3"), ("3", "-Os")],
                    );
                    self.combo_option(
                        ui,
                        "_lto_mode",
                        "LTO Mode",
                        &[
                            ("", "Default"),
                            ("no", "Disabled"),
                            ("full", "Full LTO"),
                            ("thin", "Thin LTO"),
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_llvm_ias",
                        "LLVM Integrated Assembler",
                        &[("0", "Disabled"), ("1", "Enabled")],
                    );
                    self.checkbox_option(ui, "_libunwind_replace", "Replace libunwind with llvm-libunwind");
                    self.text_option(ui, "CUSTOM_GCC_PATH", "Custom GCC Path");
                    self.text_option(ui, "CUSTOM_LLVM_PATH", "Custom LLVM Path");
                    self.text_option(ui, "KCFLAGS", "Extra KCFLAGS");
                    self.text_option(ui, "KCPPFLAGS", "Extra KCPPFLAGS");
                });

            // Kernel Version & Source
            egui::CollapsingHeader::new("Kernel Version & Source")
                .default_open(true)
                .show(ui, |ui| {
                    self.text_option(ui, "_version", "Kernel Version");
                    self.combo_option(
                        ui,
                        "_git_mirror",
                        "Git Mirror",
                        &[
                            ("", "kernel.org (default)"),
                            ("kernel.org", "kernel.org"),
                            ("googlesource.com", "googlesource.com"),
                            ("gregkh", "gregkh"),
                            ("torvalds", "torvalds"),
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_distro",
                        "Distribution",
                        &[
                            ("", "Default / prompt"),
                            ("Arch", "Arch"),
                            ("Ubuntu", "Ubuntu"),
                            ("Debian", "Debian"),
                            ("Fedora", "Fedora"),
                            ("Suse", "Suse"),
                            ("Gentoo", "Gentoo"),
                            ("Generic", "Generic"),
                        ],
                    );
                    self.text_option(ui, "_EXT_CONFIG_PATH", "External Config Path");
                    self.text_option(ui, "_kernel_work_folder", "Kernel Work Folder");
                    self.text_option(ui, "_kernel_source_folder", "Kernel Source Folder");
                    self.text_option(ui, "_custom_pkgbase", "Custom Pkgbase (Arch)");
                    self.text_option(ui, "_kernel_localversion", "Kernel Localversion");
                });

            // CPU & Performance
            egui::CollapsingHeader::new("CPU & Performance")
                .default_open(false)
                .show(ui, |ui| {
                    self.combo_option(
                        ui,
                        "_processor_opt",
                        "Processor Optimization",
                        &[
                            ("", "Default / prompt"),
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
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_timer_freq",
                        "Timer Frequency",
                        &[
                            ("", "Default / prompt"),
                            ("100", "100 Hz"),
                            ("250", "250 Hz"),
                            ("300", "300 Hz"),
                            ("500", "500 Hz"),
                            ("750", "750 Hz"),
                            ("1000", "1000 Hz"),
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_tickless",
                        "Tickless Mode",
                        &[
                            ("", "Default / prompt"),
                            ("0", "Periodic"),
                            ("1", "Full"),
                            ("2", "Idle"),
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_tcp_cong_alg",
                        "TCP Congestion Algorithm",
                        &[
                            ("", "Default (cubic)"),
                            ("yeah", "YeAH"),
                            ("bbr", "BBR"),
                            ("cubic", "CUBIC"),
                            ("reno", "Reno"),
                            ("vegas", "Vegas"),
                            ("westwood", "Westwood"),
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_default_cpu_gov",
                        "Default CPU Governor",
                        &[
                            ("", "Default (schedutil)"),
                            ("performance", "Performance"),
                            ("ondemand", "Ondemand"),
                            ("schedutil", "Schedutil"),
                        ],
                    );
                    self.checkbox_option(ui, "_aggressive_ondemand", "Aggressive Ondemand Governor");
                    self.text_option(ui, "_custom_commandline", "Default Kernel Command Line");
                    self.text_option(ui, "_NR_CPUS_value", "Max CPUs (NR_CPUS)");
                });

            // Configuration Management
            egui::CollapsingHeader::new("Configuration Management")
                .default_open(false)
                .show(ui, |ui| {
                    self.text_option(ui, "_configfile", "Config File Path");
                    self.combo_option(
                        ui,
                        "_config_updating",
                        "Config Updating",
                        &[
                            ("olddefconfig", "olddefconfig"),
                            ("oldconfig", "oldconfig"),
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_menunconfig",
                        "Menu Config",
                        &[
                            ("", "Default / prompt"),
                            ("0", "Disabled"),
                            ("1", "menuconfig"),
                            ("2", "nconfig"),
                            ("3", "xconfig"),
                        ],
                    );
                    self.checkbox_option(ui, "_diffconfig", "Generate Diffconfig Fragment");
                    self.text_option(ui, "_diffconfig_name", "Diffconfig Filename");
                    self.checkbox_option(ui, "_offline", "Offline Mode");
                    self.checkbox_option(ui, "_config_fragments", "Config Fragments (.myfrag)");
                    self.checkbox_option(
                        ui,
                        "_config_fragments_no_confirm",
                        "Skip Config Fragments Confirm",
                    );
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
                    self.checkbox_option(ui, "_preempt_rt_force", "Force PREEMPT_RT (unsupported subver)");
                    self.checkbox_option(ui, "_fsync_backport", "Fsync Backport");
                    self.checkbox_option(ui, "_fsync_legacy", "Fsync Legacy");
                    self.checkbox_option(ui, "_fsync_futex2", "Fsync futex2");
                    self.checkbox_option(ui, "_ntsync", "NTSync");
                    self.checkbox_option(ui, "_zenify", "Zenify");
                    self.checkbox_option(ui, "_glitched_base", "Glitched Base");
                    self.checkbox_option(ui, "_mglru", "MGLRU (Multi-Gen LRU)");
                    self.checkbox_option(ui, "_irq_threading", "Force IRQ Threading");
                    self.checkbox_option(ui, "_smt_nice", "SMT Nice");
                    self.checkbox_option(ui, "_random_trust_cpu", "Trust CPU RNG");
                    self.checkbox_option(ui, "_zfsfix", "ZFS FPU Export Fix (legacy)");
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
                    self.checkbox_option(ui, "_NUKR", "NUKR (cleanup after build)");
                    self.checkbox_option(ui, "_force_all_threads", "Force All Threads");
                    self.checkbox_option(ui, "_noccache", "Disable ccache");
                    self.combo_option(
                        ui,
                        "_install_after_building",
                        "Install After Building",
                        &[
                            ("prompt", "Prompt"),
                            ("yes", "Yes"),
                            ("true", "True"),
                            ("no", "No"),
                            ("false", "False"),
                        ],
                    );
                    self.combo_option(
                        ui,
                        "_logging_use_script",
                        "Use script for Logging",
                        &[("yes", "Yes"), ("no", "No")],
                    );
                });
        });
    }

    fn load_config(&mut self, path: &Path) {
        match ConfigManager::load(path) {
            Ok(manager) => {
                self.values = manager.get_all_options();
                // Migrate legacy GUI-only typo if present
                if let Some(v) = self.values.remove("_rqshare") {
                    self.values
                        .entry("_runqueue_sharing".to_string())
                        .or_insert(v);
                }
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

    fn save_config(&mut self, path: &Path, linux_tkg_path: &Path) {
        // Persist GUI feature presets into the value map before write
        let presets = FeaturePresets::from_map(&self.values);
        presets.apply_to_map(&mut self.values);

        match ConfigManager::load(path) {
            Ok(mut manager) => {
                for (key, value) in &self.values {
                    // Skip pure GUI keys that are not linux-tkg options —
                    // still write them so they reload; linux-tkg ignores unknowns.
                    manager.set_option(key, value);
                }
                match manager.save() {
                    Ok(()) => {
                        self.dirty = false;
                        match fragment_manager::sync_fragments(linux_tkg_path, &presets) {
                            Ok(actions) => {
                                self.fragment_status = if actions.is_empty() {
                                    "Fragments: no changes".into()
                                } else {
                                    format!("Fragments: {}", actions.join(", "))
                                };
                                self.status = "Config saved".to_string();
                            }
                            Err(e) => {
                                self.status = format!("Config saved; fragment error: {}", e);
                            }
                        }
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

    pub fn sync_feature_fragments(&mut self, linux_tkg_path: &Path) {
        let presets = FeaturePresets::from_map(&self.values);
        presets.apply_to_map(&mut self.values);
        match fragment_manager::sync_fragments(linux_tkg_path, &presets) {
            Ok(actions) => {
                self.fragment_status = if actions.is_empty() {
                    "Fragments: no changes".into()
                } else {
                    format!("Fragments: {}", actions.join(", "))
                };
                self.dirty = true;
            }
            Err(e) => {
                self.fragment_status = format!("Fragment error: {}", e);
            }
        }
    }

    /// Prepare linux-tkg dir before build: ensure fragments match GUI toggles.
    #[allow(dead_code)]
    pub fn prepare_for_build(&mut self, linux_tkg_path: &Path) -> Result<(), String> {
        let config_path = linux_tkg_path.join("customization.cfg");
        if !self.loaded {
            self.load_config(&config_path);
        }
        let presets = FeaturePresets::from_map(&self.values);
        // Re-save so _config_fragments_no_confirm is set when presets on
        if presets.xen_dom0 || presets.lvm_thin || presets.acpi_call {
            self.save_config(&config_path, linux_tkg_path);
        } else {
            fragment_manager::sync_fragments(linux_tkg_path, &presets)?;
        }
        Ok(())
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
        let mut checked = value == "true" || value == "1" || value == "yes";
        if ui.checkbox(&mut checked, label).changed() {
            self.values.insert(
                key.to_string(),
                if checked {
                    "true".to_string()
                } else {
                    "false".to_string()
                },
            );
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
        self.save_config(&config_path, linux_tkg_path);
    }
}
