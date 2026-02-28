use crate::core::kernel_downloader::{self, DownloadProgress};
use crate::core::kernel_fetcher::{
    self, get_previous_version, CommitInfo, FetchResult, ShortlogResult, VersionInfo,
};
use egui::{Context, RichText, Ui};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::thread;

pub struct KernelTab {
    versions: Vec<VersionInfo>,
    filter: String,
    pub selected: Option<String>,
    fetch_rx: Option<Receiver<FetchResult>>,
    shortlog_rx: Option<Receiver<ShortlogResult>>,
    status: String,
    // Detail panel state
    shortlog: Vec<CommitInfo>,
    shortlog_status: String,
    comparing_versions: Option<(String, String)>,
    // Download state
    download_rx: Option<Receiver<DownloadProgress>>,
    download_status: String,
    download_progress: Option<(u64, Option<u64>)>, // (downloaded, total)
    downloaded_path: Option<PathBuf>,
}

impl Default for KernelTab {
    fn default() -> Self {
        Self {
            versions: Vec::new(),
            filter: String::new(),
            selected: None,
            fetch_rx: None,
            shortlog_rx: None,
            status: "Click 'Refresh' to fetch kernel versions".to_string(),
            shortlog: Vec::new(),
            shortlog_status: String::new(),
            comparing_versions: None,
            download_rx: None,
            download_status: String::new(),
            download_progress: None,
            downloaded_path: None,
        }
    }
}

impl KernelTab {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &Context, kernel_sources_dir: &Path) {
        // Drain any pending fetch results
        let mut should_clear_fetch_rx = false;
        if let Some(rx) = &self.fetch_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    FetchResult::Done(versions) => {
                        self.status = format!("{} versions loaded", versions.len());
                        self.versions = versions;
                    }
                    FetchResult::Error(e) => {
                        self.status = format!("Error: {}", e);
                    }
                }
                should_clear_fetch_rx = true;
            }
        }
        if should_clear_fetch_rx {
            self.fetch_rx = None;
        }

        // Drain shortlog results
        let mut should_clear_shortlog_rx = false;
        if let Some(rx) = &self.shortlog_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    ShortlogResult::Done(commits) => {
                        self.shortlog_status = format!("{} commits", commits.len());
                        self.shortlog = commits;
                    }
                    ShortlogResult::Error(e) => {
                        self.shortlog_status = format!("Error: {}", e);
                        self.shortlog.clear();
                    }
                }
                should_clear_shortlog_rx = true;
            }
        }
        if should_clear_shortlog_rx {
            self.shortlog_rx = None;
        }

        // Drain download progress updates
        let mut should_clear_download_rx = false;
        if let Some(rx) = &self.download_rx {
            while let Ok(progress) = rx.try_recv() {
                match progress {
                    DownloadProgress::Started(total) => {
                        self.download_status = "Downloading...".to_string();
                        self.download_progress = Some((0, total));
                    }
                    DownloadProgress::Downloading(downloaded) => {
                        if let Some((_, total)) = &self.download_progress {
                            self.download_progress = Some((downloaded, *total));
                        }
                    }
                    DownloadProgress::Extracting => {
                        self.download_status = "Extracting...".to_string();
                        self.download_progress = None;
                    }
                    DownloadProgress::Complete(path) => {
                        self.download_status = format!("✓ Downloaded to: {}", path.display());
                        self.downloaded_path = Some(path);
                        self.download_progress = None;
                        should_clear_download_rx = true;
                    }
                    DownloadProgress::Error(e) => {
                        self.download_status = format!("✗ Error: {}", e);
                        self.download_progress = None;
                        should_clear_download_rx = true;
                    }
                }
            }
        }
        if should_clear_download_rx {
            self.download_rx = None;
        }

        ui.heading("🐧 Kernel Version Browser");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.fetch_rx.is_none(), egui::Button::new("🔄 Refresh"))
                .clicked()
            {
                self.start_fetch(ctx.clone());
            }
            ui.label(&self.status);
        });

        ui.add_space(8.0);

        // Split into two columns: version list and detail panel
        ui.columns(2, |cols| {
            // Left column: version list
            cols[0].horizontal(|ui| {
                ui.label("Filter:");
                ui.text_edit_singleline(&mut self.filter);
            });

            cols[0].add_space(4.0);

            let filter_lower = self.filter.to_lowercase();
            let filtered: Vec<_> = self
                .versions
                .iter()
                .filter(|v| v.version.to_lowercase().contains(&filter_lower))
                .collect();

            cols[0].label(format!(
                "Showing {} of {} versions",
                filtered.len(),
                self.versions.len()
            ));

            cols[0].add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("version_list")
                .show(&mut cols[0], |ui| {
                    for info in &filtered {
                        let is_selected = self.selected.as_ref() == Some(&info.version);
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(is_selected, &info.version)
                                .clicked()
                            {
                                let version_changed = self.selected.as_ref() != Some(&info.version);
                                self.selected = Some(info.version.clone());
                                if version_changed {
                                    self.shortlog.clear();
                                    self.shortlog_status.clear();
                                    self.comparing_versions = None;
                                }
                            }
                            if let Some(date) = &info.date {
                                ui.label(
                                    RichText::new(date).small().color(egui::Color32::GRAY),
                                );
                            }
                        });
                    }
                });

            // Right column: detail panel
            self.detail_panel(&mut cols[1], ctx, kernel_sources_dir);
        });
    }

    fn detail_panel(&mut self, ui: &mut Ui, ctx: &Context, kernel_sources_dir: &Path) {
        ui.group(|ui| {
            if let Some(selected) = &self.selected.clone() {
                ui.heading(format!("📋 {}", selected));

                // Find version info
                if let Some(info) = self.versions.iter().find(|v| &v.version == selected) {
                    if let Some(date) = &info.date {
                        ui.label(format!("Released: {}", date));
                    }
                }

                ui.add_space(8.0);

                // Find previous version to compare against
                let prev_version = get_previous_version(selected, &self.versions);

                if let Some(prev) = &prev_version {
                    ui.label(format!("Changes since {}", prev));

                    ui.horizontal(|ui| {
                        let is_loading = self.shortlog_rx.is_some();
                        if ui
                            .add_enabled(!is_loading, egui::Button::new("🔍 Fetch Changes"))
                            .clicked()
                        {
                            self.start_shortlog_fetch(prev.clone(), selected.clone(), ctx.clone());
                        }

                        if !self.shortlog_status.is_empty() {
                            ui.label(&self.shortlog_status);
                        } else if is_loading {
                            ui.label("Fetching…");
                        }
                    });

                    // Show comparison info
                    if let Some((from, to)) = &self.comparing_versions {
                        ui.label(
                            RichText::new(format!("Comparing {} → {}", from, to))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }

                    ui.add_space(4.0);

                    // Shortlog display
                    if !self.shortlog.is_empty() {
                        egui::ScrollArea::vertical()
                            .id_salt("shortlog")
                            .max_height(350.0)
                            .show(ui, |ui| {
                                for commit in &self.shortlog {
                                    ui.horizontal(|ui| {
                                        if !commit.hash.is_empty() {
                                            ui.label(
                                                RichText::new(&commit.hash[..commit.hash.len().min(8)])
                                                    .monospace()
                                                    .color(egui::Color32::YELLOW),
                                            );
                                        }
                                        ui.label(&commit.subject);
                                    });
                                    if !commit.author.is_empty() {
                                        ui.label(
                                            RichText::new(format!("    — {}", commit.author))
                                                .small()
                                                .color(egui::Color32::GRAY),
                                        );
                                    }
                                    ui.add_space(2.0);
                                }
                            });
                    }
                } else {
                    ui.label(
                        RichText::new("Base version (no previous version in series)")
                            .color(egui::Color32::GRAY),
                    );
                }

                ui.add_space(8.0);

                // Link to kernel.org
                let url = format!(
                    "https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tag/?h={}",
                    selected
                );
                ui.hyperlink_to("View on kernel.org", url);

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Download section
                ui.heading("📥 Download Sources");
                ui.add_space(4.0);

                let download_url = kernel_downloader::get_download_url(selected);
                ui.label(
                    RichText::new(format!("From: {}", download_url))
                        .small()
                        .color(egui::Color32::GRAY),
                );

                ui.add_space(4.0);

                let is_downloading = self.download_rx.is_some();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!is_downloading, egui::Button::new("⬇ Download Kernel Sources"))
                        .clicked()
                    {
                        self.start_download(selected.clone(), ctx.clone(), kernel_sources_dir.to_path_buf());
                    }
                });

                // Show download progress
                if let Some((downloaded, total)) = &self.download_progress {
                    ui.add_space(4.0);
                    if let Some(total) = total {
                        let progress = *downloaded as f32 / *total as f32;
                        ui.add(egui::ProgressBar::new(progress).show_percentage());
                        ui.label(format!(
                            "{} / {}",
                            kernel_downloader::format_bytes(*downloaded),
                            kernel_downloader::format_bytes(*total)
                        ));
                    } else {
                        ui.label(format!(
                            "Downloaded: {}",
                            kernel_downloader::format_bytes(*downloaded)
                        ));
                    }
                }

                if !self.download_status.is_empty() {
                    ui.add_space(4.0);
                    let color = if self.download_status.starts_with('✓') {
                        egui::Color32::GREEN
                    } else if self.download_status.starts_with('✗') {
                        egui::Color32::RED
                    } else {
                        egui::Color32::YELLOW
                    };
                    ui.label(RichText::new(&self.download_status).color(color));
                }

                if let Some(path) = &self.downloaded_path {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("Ready for build at: {}", path.display()))
                            .small()
                            .color(egui::Color32::LIGHT_GREEN),
                    );
                }
            } else {
                ui.label("Select a version to see details");
            }
        });
    }

    fn start_fetch(&mut self, ctx: Context) {
        self.status = "Fetching…".to_string();
        let (tx, rx) = channel();
        self.fetch_rx = Some(rx);

        thread::spawn(move || {
            let result = kernel_fetcher::fetch_tags();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn start_shortlog_fetch(&mut self, from: String, to: String, ctx: Context) {
        self.shortlog_status = "Fetching…".to_string();
        self.shortlog.clear();
        self.comparing_versions = Some((from.clone(), to.clone()));

        let (tx, rx) = channel();
        self.shortlog_rx = Some(rx);

        thread::spawn(move || {
            let result = kernel_fetcher::fetch_shortlog(&from, &to);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn start_download(&mut self, version: String, ctx: Context, kernel_sources_dir: PathBuf) {
        self.download_status = "Starting download...".to_string();
        self.download_progress = None;
        self.downloaded_path = None;

        let (tx, rx) = channel();
        self.download_rx = Some(rx);

        thread::spawn(move || {
            let dest_dir = kernel_sources_dir;

            // Spawn a repaint thread to keep UI updated during download
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::Arc;

            let running = Arc::new(AtomicBool::new(true));
            let running_clone = running.clone();
            let ctx_clone = ctx.clone();

            let repaint_handle = thread::spawn(move || {
                while running_clone.load(Ordering::Relaxed) {
                    thread::sleep(std::time::Duration::from_millis(100));
                    ctx_clone.request_repaint();
                }
            });

            let _ = kernel_downloader::download_kernel(&version, &dest_dir, tx);

            // Stop the repaint thread
            running.store(false, Ordering::Relaxed);
            let _ = repaint_handle.join();
            ctx.request_repaint();
        });
    }

    pub fn get_selected_version(&self) -> Option<String> {
        self.selected.clone()
    }

    /// Extract major.minor from version string (e.g., "v6.13.1" -> "6.13")
    pub fn get_kernel_series(&self) -> Option<String> {
        self.selected.as_ref().map(|v| {
            let stripped = v.trim_start_matches('v');
            let parts: Vec<&str> = stripped.split('.').collect();
            if parts.len() >= 2 {
                format!("{}.{}", parts[0], parts[1])
            } else {
                stripped.to_string()
            }
        })
    }
}
