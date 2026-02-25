use crate::core::patch_manager::{
    delete_patch, download_patch, extract_filename_from_url, get_patch_dir, list_patches,
    toggle_patch, DownloadInfo, DownloadResult, PatchEntry,
};
use crate::core::patch_registry::{
    check_update, PatchMeta, PatchRegistry, UpdateCheckResult, UpdateStatus,
};
use crate::data::catalog::{catalog_for_series, CatalogEntry};
use chrono::Utc;
use egui::{Color32, Context, RichText, Ui};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::thread;

pub struct PatchesTab {
    // URL download
    url_input: String,
    filename_input: String,
    kernel_series: String,
    patches: Vec<PatchEntry>,
    download_rx: Option<Receiver<DownloadResult>>,
    status: String,
    last_url: String,

    // Registry and catalog
    registry: PatchRegistry,
    catalog_filter: String,
    update_rx: Option<Receiver<UpdateCheckResult>>,
    update_status: String,

    // Track pending download metadata
    pending_download: Option<PendingDownload>,

    // Track last data_dir to detect changes and reload registry
    last_data_dir: Option<PathBuf>,
}

struct PendingDownload {
    url: String,
    catalog_id: Option<String>,
}

impl Default for PatchesTab {
    fn default() -> Self {
        Self {
            url_input: String::new(),
            filename_input: String::new(),
            kernel_series: "6.13".to_string(),
            patches: Vec::new(),
            download_rx: None,
            status: String::new(),
            last_url: String::new(),
            registry: PatchRegistry::default(),
            catalog_filter: String::new(),
            update_rx: None,
            update_status: String::new(),
            pending_download: None,
            last_data_dir: None,
        }
    }
}

impl PatchesTab {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &Context, linux_tkg_path: &Path, data_dir: &Path) {
        // Reload registry if data_dir changed
        if self.last_data_dir.as_deref() != Some(data_dir) {
            self.registry = PatchRegistry::load(data_dir);
            self.last_data_dir = Some(data_dir.to_path_buf());
        }

        // Drain download results
        let mut download_complete = false;
        if let Some(rx) = &self.download_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    DownloadResult::Done(info) => {
                        self.handle_download_complete(info, data_dir);
                        self.refresh_patches(linux_tkg_path);
                        download_complete = true;
                    }
                    DownloadResult::Error(e) => {
                        self.status = format!("Error: {}", e);
                        download_complete = true;
                    }
                }
            }
        }
        if download_complete {
            self.download_rx = None;
            self.pending_download = None;
        }

        // Drain update check results
        let mut updates_to_apply: Vec<(String, UpdateStatus)> = Vec::new();
        if let Some(rx) = &self.update_rx {
            while let Ok(result) = rx.try_recv() {
                match result {
                    UpdateCheckResult::UpToDate { key } => {
                        updates_to_apply.push((key, UpdateStatus::UpToDate));
                    }
                    UpdateCheckResult::Stale { key } => {
                        updates_to_apply.push((key, UpdateStatus::Stale));
                    }
                    UpdateCheckResult::Error { key, reason } => {
                        updates_to_apply.push((key, UpdateStatus::CheckError(reason)));
                    }
                    UpdateCheckResult::NoUrl { key } => {
                        updates_to_apply.push((key, UpdateStatus::Unknown));
                    }
                }
            }
        }

        // Apply updates
        for (key, status) in updates_to_apply {
            if let Some((series, filename)) = key.split_once('/') {
                self.registry.update_status(series, filename, status);
            }
        }

        // Auto-fill filename from URL
        if self.url_input != self.last_url {
            self.filename_input = extract_filename_from_url(&self.url_input);
            self.last_url = self.url_input.clone();
        }

        ui.heading("🩹 Patch Management");

        ui.horizontal(|ui| {
            ui.label("Kernel Series:");
            ui.add(egui::TextEdit::singleline(&mut self.kernel_series).desired_width(60.0));
        });

        ui.add_space(8.0);

        // Catalog section
        egui::CollapsingHeader::new("📦 Available Patches (Catalog)")
            .default_open(true)
            .show(ui, |ui| {
                self.catalog_ui(ui, ctx, linux_tkg_path, data_dir);
            });

        ui.add_space(8.0);

        // URL download section
        egui::CollapsingHeader::new("🔗 Download from URL")
            .default_open(false)
            .show(ui, |ui| {
                self.url_download_ui(ui, ctx, linux_tkg_path, data_dir);
            });

        ui.add_space(8.0);

        // Installed patches section
        egui::CollapsingHeader::new("📂 Installed Patches")
            .default_open(true)
            .show(ui, |ui| {
                self.installed_patches_ui(ui, ctx, linux_tkg_path, data_dir);
            });
    }

    fn catalog_ui(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        linux_tkg_path: &Path,
        data_dir: &Path,
    ) {
        ui.horizontal(|ui| {
            ui.label("🔍");
            ui.add(
                egui::TextEdit::singleline(&mut self.catalog_filter)
                    .hint_text("Filter catalog...")
                    .desired_width(200.0),
            );
        });

        ui.add_space(4.0);

        let catalog = catalog_for_series(&self.kernel_series);
        let filter_lower = self.catalog_filter.to_lowercase();

        if catalog.is_empty() {
            ui.label(
                RichText::new(format!(
                    "No catalog patches available for kernel {}",
                    self.kernel_series
                ))
                .color(Color32::GRAY),
            );
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("catalog")
            .max_height(200.0)
            .show(ui, |ui| {
                for entry in catalog {
                    if !filter_lower.is_empty()
                        && !entry.name.to_lowercase().contains(&filter_lower)
                        && !entry.description.to_lowercase().contains(&filter_lower)
                    {
                        continue;
                    }

                    let filename = entry.filename_for_series(&self.kernel_series);
                    let is_installed = self.patches.iter().any(|p| p.name == filename);

                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(entry.name);

                            if is_installed {
                                ui.label(RichText::new("✓ installed").color(Color32::GREEN));
                            } else {
                                let is_downloading = self.download_rx.is_some();
                                if ui
                                    .add_enabled(
                                        !is_downloading,
                                        egui::Button::new("⬇ Download"),
                                    )
                                    .clicked()
                                {
                                    self.start_catalog_download(
                                        entry,
                                        linux_tkg_path,
                                        data_dir,
                                        ctx.clone(),
                                    );
                                }
                            }
                        });
                        ui.label(
                            RichText::new(entry.description)
                                .small()
                                .color(Color32::GRAY),
                        );
                    });
                }
            });
    }

    fn url_download_ui(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        linux_tkg_path: &Path,
        _data_dir: &Path,
    ) {
        ui.horizontal(|ui| {
            ui.label("URL:");
            ui.add(egui::TextEdit::singleline(&mut self.url_input).desired_width(400.0));
        });

        ui.horizontal(|ui| {
            ui.label("Filename:");
            ui.add(
                egui::TextEdit::singleline(&mut self.filename_input).desired_width(200.0),
            );
        });

        ui.horizontal(|ui| {
            let can_download = self.download_rx.is_none()
                && !self.url_input.is_empty()
                && !self.filename_input.is_empty();

            if ui
                .add_enabled(can_download, egui::Button::new("⬇ Download"))
                .clicked()
            {
                self.start_url_download(linux_tkg_path, ctx.clone());
            }

            if !self.status.is_empty() {
                ui.label(&self.status);
            }
        });
    }

    fn installed_patches_ui(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        linux_tkg_path: &Path,
        data_dir: &Path,
    ) {
        let patch_dir = get_patch_dir(linux_tkg_path, &self.kernel_series);

        ui.horizontal(|ui| {
            ui.label(format!("Dir: {}", patch_dir.display()));
        });

        ui.horizontal(|ui| {
            if ui.button("📂 Open in File Manager").clicked() {
                let _ = std::process::Command::new("xdg-open")
                    .arg(&patch_dir)
                    .spawn();
            }

            if ui.button("🔄 Refresh").clicked() {
                self.refresh_patches(linux_tkg_path);
            }

            let has_checkable = self.patches.iter().any(|p| {
                self.registry
                    .get(&self.kernel_series, &p.name)
                    .map(|m| m.source_url.is_some())
                    .unwrap_or(false)
            });

            if ui
                .add_enabled(
                    has_checkable && self.update_rx.is_none(),
                    egui::Button::new("🔍 Check All for Updates"),
                )
                .clicked()
            {
                self.check_all_updates(ctx.clone());
            }

            if !self.update_status.is_empty() {
                ui.label(&self.update_status);
            }
        });

        ui.add_space(8.0);

        if self.patches.is_empty() {
            ui.label("No patches installed for this kernel series");
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("installed")
            .max_height(300.0)
            .show(ui, |ui| {
                let mut to_toggle: Option<usize> = None;
                let mut to_delete: Option<usize> = None;
                let mut to_redownload: Option<String> = None;
                let mut to_check: Option<PatchMeta> = None;

                for (i, patch) in self.patches.iter().enumerate() {
                    let meta = self.registry.get(&self.kernel_series, &patch.name);

                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            // Enable/disable toggle
                            let enabled_text = if patch.enabled { "✓" } else { "✗" };
                            let color = if patch.enabled {
                                Color32::GREEN
                            } else {
                                Color32::GRAY
                            };

                            if ui
                                .button(RichText::new(enabled_text).color(color))
                                .on_hover_text(if patch.enabled {
                                    "Click to disable"
                                } else {
                                    "Click to enable"
                                })
                                .clicked()
                            {
                                to_toggle = Some(i);
                            }

                            ui.strong(&patch.name);

                            // Update status badge
                            if let Some(meta) = meta {
                                let (badge, badge_color) = match &meta.update_status {
                                    UpdateStatus::Unknown => ("⬜", Color32::GRAY),
                                    UpdateStatus::UpToDate => ("🟢", Color32::GREEN),
                                    UpdateStatus::Stale => ("🟡", Color32::YELLOW),
                                    UpdateStatus::CheckError(_) => ("🔴", Color32::RED),
                                };
                                ui.label(RichText::new(badge).color(badge_color));
                            } else {
                                ui.label(RichText::new("⬜").color(Color32::GRAY));
                            }
                        });

                        // Metadata row
                        if let Some(meta) = meta {
                            ui.horizontal(|ui| {
                                if let Some(url) = &meta.source_url {
                                    let short_url = if url.len() > 40 {
                                        format!("{}...", &url[..40])
                                    } else {
                                        url.clone()
                                    };
                                    ui.label(
                                        RichText::new(format!("src: {}", short_url))
                                            .small()
                                            .color(Color32::GRAY),
                                    );
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{}  sha: {}...",
                                        meta.downloaded_at.format("%Y-%m-%d"),
                                        &meta.sha256[..8.min(meta.sha256.len())]
                                    ))
                                    .small()
                                    .color(Color32::GRAY),
                                );
                            });
                        }

                        // Action buttons
                        ui.horizontal(|ui| {
                            if let Some(meta) = meta {
                                if meta.source_url.is_some() {
                                    if ui.small_button("🔍 Check Update").clicked() {
                                        to_check = Some(meta.clone());
                                    }
                                    if ui.small_button("🔄 Re-download").clicked() {
                                        to_redownload = meta.source_url.clone();
                                    }
                                }
                            }

                            if ui
                                .small_button(RichText::new("🗑 Delete").color(Color32::RED))
                                .clicked()
                            {
                                to_delete = Some(i);
                            }
                        });
                    });
                }

                // Handle actions
                if let Some(i) = to_toggle {
                    if let Err(e) = toggle_patch(&mut self.patches[i]) {
                        self.status = format!("Error: {}", e);
                    }
                }

                if let Some(i) = to_delete {
                    let patch = &self.patches[i];
                    self.registry.remove(&self.kernel_series, &patch.name);
                    let _ = self.registry.save(data_dir);

                    if let Err(e) = delete_patch(patch) {
                        self.status = format!("Error: {}", e);
                    } else {
                        self.patches.remove(i);
                    }
                }

                if let Some(meta) = to_check {
                    self.check_single_update(meta, ctx.clone());
                }

                if let Some(url) = to_redownload {
                    if let Some(meta) = self
                        .registry
                        .all_for_series(&self.kernel_series)
                        .into_iter()
                        .find(|m| m.source_url.as_ref() == Some(&url))
                    {
                        self.url_input = url;
                        self.filename_input = meta.filename.clone();
                        self.pending_download = Some(PendingDownload {
                            url: self.url_input.clone(),
                            catalog_id: meta.catalog_id.clone(),
                        });
                        self.start_url_download(linux_tkg_path, ctx.clone());
                    }
                }
            });
    }

    fn start_catalog_download(
        &mut self,
        entry: &CatalogEntry,
        linux_tkg_path: &Path,
        data_dir: &Path,
        ctx: Context,
    ) {
        let url = entry.url_for_series(&self.kernel_series);
        let filename = entry.filename_for_series(&self.kernel_series);

        self.pending_download = Some(PendingDownload {
            url: url.clone(),
            catalog_id: Some(entry.id.to_string()),
        });

        let patch_dir = get_patch_dir(linux_tkg_path, &self.kernel_series);
        let dest_path = patch_dir.join(&filename);

        // Store data_dir for use when download completes (via last_data_dir)
        self.last_data_dir = Some(data_dir.to_path_buf());

        self.status = format!("Downloading {}...", entry.name);
        let (tx, rx) = channel();
        self.download_rx = Some(rx);

        thread::spawn(move || {
            let result = download_patch(&url, &dest_path);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn start_url_download(&mut self, linux_tkg_path: &Path, ctx: Context) {
        let patch_dir = get_patch_dir(linux_tkg_path, &self.kernel_series);
        let dest_path = patch_dir.join(&self.filename_input);
        let url = self.url_input.clone();

        if self.pending_download.is_none() {
            self.pending_download = Some(PendingDownload {
                url: url.clone(),
                catalog_id: None,
            });
        }

        self.status = "Downloading…".to_string();
        let (tx, rx) = channel();
        self.download_rx = Some(rx);

        thread::spawn(move || {
            let result = download_patch(&url, &dest_path);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn handle_download_complete(&mut self, info: DownloadInfo, data_dir: &Path) {
        self.status = format!("Downloaded: {}", info.path.display());

        // Get the actual filename from the path (may differ due to decompression)
        let filename = info
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Create registry entry
        let meta = PatchMeta {
            filename,
            kernel_series: self.kernel_series.clone(),
            source_url: self.pending_download.as_ref().map(|p| p.url.clone()),
            catalog_id: self
                .pending_download
                .as_ref()
                .and_then(|p| p.catalog_id.clone()),
            sha256: info.sha256,
            downloaded_at: Utc::now(),
            etag: info.etag,
            last_modified: info.last_modified,
            update_status: UpdateStatus::UpToDate,
        };

        self.registry.record_download(meta);
        let _ = self.registry.save(data_dir);
    }

    fn check_single_update(&mut self, meta: PatchMeta, ctx: Context) {
        self.update_status = "Checking...".to_string();
        let (tx, rx) = channel();
        self.update_rx = Some(rx);

        check_update(meta, tx);
        ctx.request_repaint();
    }

    fn check_all_updates(&mut self, ctx: Context) {
        let patches_with_urls: Vec<_> = self
            .patches
            .iter()
            .filter_map(|p| self.registry.get(&self.kernel_series, &p.name).cloned())
            .filter(|m| m.source_url.is_some())
            .collect();

        if patches_with_urls.is_empty() {
            self.update_status = "No patches with source URLs".to_string();
            return;
        }

        self.update_status = format!("Checking {} patches...", patches_with_urls.len());
        let (tx, rx) = channel();
        self.update_rx = Some(rx);

        for meta in patches_with_urls {
            check_update(meta, tx.clone());
        }
        ctx.request_repaint();
    }

    fn refresh_patches(&mut self, linux_tkg_path: &Path) {
        let patch_dir = get_patch_dir(linux_tkg_path, &self.kernel_series);
        self.patches = list_patches(&patch_dir);
    }

    pub fn set_kernel_series(&mut self, series: &str) {
        self.kernel_series = series.to_string();
    }
}
