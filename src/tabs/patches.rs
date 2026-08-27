use crate::core::patch_browser::{
    browse_patchsets, describe_patchset, download_patchset, BrowseResult, PatchsetResult,
    RemotePatchset,
};
use crate::core::patch_manager::{
    delete_patch, download_patch, extract_filename_from_url, get_patch_dir, list_patches,
    toggle_patch, DownloadInfo, DownloadResult, PatchEntry,
};
use crate::core::patch_registry::{
    check_update, PatchMeta, PatchRegistry, UpdateCheckResult, UpdateStatus,
};
use crate::settings::default_patch_repo;
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

    // Registry
    registry: PatchRegistry,
    update_rx: Option<Receiver<UpdateCheckResult>>,
    update_status: String,

    // Live patch browser (configurable GitHub owner/repo)
    patch_repo: String,
    browse_rx: Option<Receiver<BrowseResult>>,
    browse_results: Vec<RemotePatchset>,
    browse_filter: String,
    browse_status: String,
    patchset_rx: Option<Receiver<PatchsetResult>>,
    patchset_downloading: Option<String>,

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
            update_rx: None,
            update_status: String::new(),
            patch_repo: default_patch_repo(),
            browse_rx: None,
            browse_results: Vec::new(),
            browse_filter: String::new(),
            browse_status: String::new(),
            patchset_rx: None,
            patchset_downloading: None,
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
        let mut updates_done = false;
        if let Some(rx) = &self.update_rx {
            loop {
                match rx.try_recv() {
                    Ok(result) => match result {
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
                    },
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    // All senders dropped: all threads have completed
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        updates_done = true;
                        break;
                    }
                }
            }
        }
        if updates_done {
            self.update_rx = None;
            self.update_status = "Update check complete.".to_string();
        }

        // Apply updates
        for (key, status) in updates_to_apply {
            if let Some((series, filename)) = key.split_once('/') {
                self.registry.update_status(series, filename, status);
            }
        }

        // Drain patchset-browser list results
        if let Some(rx) = &self.browse_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    BrowseResult::Done(sets) => {
                        self.browse_status = format!("{} patchsets available", sets.len());
                        self.browse_results = sets;
                    }
                    BrowseResult::Error(e) => {
                        self.browse_status = format!("Error: {}", e);
                        self.browse_results.clear();
                    }
                }
                self.browse_rx = None;
            }
        }

        // Drain patchset download results
        if let Some(rx) = &self.patchset_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    PatchsetResult::Done(installed) => {
                        let count = installed.len();
                        for ip in installed {
                            self.registry.record_download(PatchMeta {
                                filename: ip.filename,
                                kernel_series: self.kernel_series.clone(),
                                source_url: Some(ip.source_url),
                                catalog_id: None,
                                sha256: ip.info.sha256,
                                downloaded_at: Utc::now(),
                                etag: ip.info.etag,
                                last_modified: ip.info.last_modified,
                                update_status: UpdateStatus::UpToDate,
                            });
                        }
                        let _ = self.registry.save(data_dir);
                        self.refresh_patches(linux_tkg_path);
                        let name = self.patchset_downloading.take().unwrap_or_default();
                        self.browse_status = format!("Installed {} file(s) from {}", count, name);
                    }
                    PatchsetResult::Error(e) => {
                        self.browse_status = format!("Error: {}", e);
                    }
                }
                self.patchset_rx = None;
                self.patchset_downloading = None;
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

        // Live browser section
        egui::CollapsingHeader::new(format!("🌐 Browse Patches ({})", self.patch_repo))
            .default_open(true)
            .show(ui, |ui| {
                self.browse_ui(ui, ctx, linux_tkg_path);
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

    fn browse_ui(&mut self, ui: &mut Ui, ctx: &Context, linux_tkg_path: &Path) {
        ui.horizontal(|ui| {
            let loading = self.browse_rx.is_some();
            if ui
                .add_enabled(!loading, egui::Button::new("🔄 Refresh List"))
                .on_hover_text(format!(
                    "Fetch patchsets for kernel {} from {}",
                    self.kernel_series, self.patch_repo
                ))
                .clicked()
            {
                self.start_browse(ctx.clone());
            }

            if loading {
                ui.spinner();
                ui.label("Fetching patch list…");
            } else if !self.browse_status.is_empty() {
                ui.label(&self.browse_status);
            }
        });

        if self.browse_results.is_empty() {
            ui.label(
                RichText::new(
                    "Click “Refresh List” to load the patchsets available for this kernel series.",
                )
                .color(Color32::GRAY),
            );
            return;
        }

        ui.horizontal(|ui| {
            ui.label("🔍");
            ui.add(
                egui::TextEdit::singleline(&mut self.browse_filter)
                    .hint_text("Filter patchsets…")
                    .desired_width(200.0),
            );
        });

        ui.add_space(4.0);

        let filter = self.browse_filter.to_lowercase();
        let busy = self.patchset_rx.is_some();
        let mut to_download: Option<RemotePatchset> = None;

        egui::ScrollArea::vertical()
            .id_salt("browse")
            .max_height(260.0)
            .show(ui, |ui| {
                for set in &self.browse_results {
                    if !filter.is_empty() && !set.name.to_lowercase().contains(&filter) {
                        continue;
                    }

                    let downloading_this =
                        self.patchset_downloading.as_deref() == Some(set.name.as_str());

                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            if downloading_this {
                                ui.spinner();
                            } else if ui
                                .add_enabled(!busy, egui::Button::new("⬇ Download"))
                                .on_hover_text("Download all files in this set as .mypatch")
                                .clicked()
                            {
                                to_download = Some(set.clone());
                            }
                            ui.strong(&set.name);
                        });
                        if let Some(hint) = describe_patchset(&set.name) {
                            ui.label(RichText::new(hint).small().color(Color32::GRAY));
                        }
                    });
                }
            });

        if let Some(set) = to_download {
            self.start_patchset_download(set, linux_tkg_path, ctx.clone());
        }
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
            ui.add(egui::TextEdit::singleline(&mut self.filename_input).desired_width(200.0));
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
                    match delete_patch(patch) {
                        Err(e) => {
                            self.status = format!("Error: {}", e);
                        }
                        Ok(()) => {
                            // Only remove from registry after successful file deletion
                            self.registry.remove(&self.kernel_series, &patch.name);
                            let _ = self.registry.save(data_dir);
                            self.patches.remove(i);
                        }
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

    fn start_browse(&mut self, ctx: Context) {
        let series = self.kernel_series.clone();
        let repo = self.patch_repo.clone();
        self.browse_status.clear();
        let (tx, rx) = channel();
        self.browse_rx = Some(rx);

        thread::spawn(move || {
            let result = browse_patchsets(&repo, &series);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn start_patchset_download(
        &mut self,
        set: RemotePatchset,
        linux_tkg_path: &Path,
        ctx: Context,
    ) {
        let patch_dir = get_patch_dir(linux_tkg_path, &self.kernel_series);
        let repo = self.patch_repo.clone();
        self.patchset_downloading = Some(set.name.clone());
        self.browse_status = format!("Downloading {}…", set.name);

        let (tx, rx) = channel();
        self.patchset_rx = Some(rx);

        thread::spawn(move || {
            let result = download_patchset(&repo, &set.path, &set.name, &patch_dir);
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

    /// Sync the browsed GitHub `owner/repo` from app settings. Clears any stale
    /// browse results when the repo changes so the list isn't misleading.
    pub fn set_patch_repo(&mut self, repo: &str) {
        if self.patch_repo != repo {
            self.patch_repo = repo.to_string();
            self.browse_results.clear();
            self.browse_status.clear();
        }
    }
}
