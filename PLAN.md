# TKG GUI — Implementation Plan (Rust)

## Overview

A Rust desktop GUI for building custom Linux kernels via linux-tkg. Four main tabs:
1. **Kernel** — Browse/search kernel versions from git.kernel.org
2. **Config** — Edit all `customization.cfg` options via dropdowns/checkboxes
3. **Patches** — Download and manage custom user patches
4. **Build** — Run `makepkg` and stream live log output

---

## Technology Stack

| Component | Crate |
|---|---|
| GUI framework | `egui` + `eframe` |
| HTTP client | `ureq` (blocking, zero-dependency) |
| HTML parsing | `scraper` (CSS selectors) |
| Regex | `regex` |
| Subprocess | `std::process::Command` + `std::sync::mpsc` |
| Async coordination | `std::thread` + `std::sync::mpsc` channels |
| Serialization | `serde` + `serde_json` |
| Integrity hashing | `sha2` |

**Why egui/eframe:** Immediate-mode rendering maps naturally onto streaming log output
(just append to a `Vec` and repaint). No system dependencies — ships as a single static
binary. Background work (HTTP fetches, subprocess I/O) runs in `std::thread`s and
communicates back via `mpsc` channels; the UI drains channels on every frame.

---

## Repository Layout

```
tkg-gui/
├── Cargo.toml
├── Cargo.lock
├── .tkg-gui/
│   └── patch_registry.json    # auto-created; gitignored; tracks installed patch metadata
├── src/
│   ├── main.rs                # eframe::run_native() entry point
│   ├── app.rs                 # TkgApp struct + eframe::App impl
│   ├── tabs/
│   │   ├── mod.rs
│   │   ├── kernel.rs          # Kernel version browser tab
│   │   ├── config.rs          # Config options tab
│   │   ├── patches.rs         # Patch download/manage tab (enhanced)
│   │   └── build.rs           # makepkg runner + log tab
│   ├── core/
│   │   ├── mod.rs
│   │   ├── kernel_fetcher.rs  # HTTP fetch + HTML parse git.kernel.org tags
│   │   ├── config_manager.rs  # Parse/write customization.cfg
│   │   ├── patch_manager.rs   # Download patches, manage patch dirs, compute SHA-256
│   │   ├── patch_registry.rs  # Load/save patch_registry.json; update-check via HTTP HEAD
│   │   └── build_manager.rs   # Subprocess spawn + line-by-line channel output
│   └── data/
│       ├── mod.rs
│       └── catalog.rs         # Hardcoded Vec<CatalogEntry> of known userpatch sources
└── submodules/
    ├── linux-tkg/              # git submodule (Frogging-Family/linux-tkg)
    └── wine-tkg-git/           # git submodule (Frogging-Family/wine-tkg-git)
```

Add `.tkg-gui/` to `.gitignore` so the local patch registry is not committed.

---

## Submodules

Update `.gitmodules` to add `wine-tkg-git` and remove the duplicate root-level entry:

```ini
[submodule "submodules/linux-tkg"]
    path = submodules/linux-tkg
    url = https://github.com/Frogging-Family/linux-tkg

[submodule "submodules/wine-tkg-git"]
    path = submodules/wine-tkg-git
    url = https://github.com/Frogging-Family/wine-tkg-git
```

---

## `Cargo.toml`

```toml
[package]
name = "tkg-gui"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "tkg-gui"
path = "src/main.rs"

[dependencies]
eframe = "0.29"
egui  = "0.29"
ureq  = "2"
scraper = "0.20"
regex = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
```

---

## Application State (`src/app.rs`)

```rust
#[derive(PartialEq)]
enum Tab { Kernel, Config, Patches, Build }

pub struct TkgApp {
    active_tab: Tab,
    kernel_tab: KernelTab,
    config_tab: ConfigTab,
    patches_tab: PatchesTab,
    build_tab: BuildTab,
    base_dir: PathBuf,   // repo root (tkg-gui/)
}

impl eframe::App for TkgApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, Tab::Kernel,  "🐧 Kernel");
                ui.selectable_value(&mut self.active_tab, Tab::Config,  "⚙  Config");
                ui.selectable_value(&mut self.active_tab, Tab::Patches, "🩹 Patches");
                ui.selectable_value(&mut self.active_tab, Tab::Build,   "🔨 Build");
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                Tab::Kernel  => self.kernel_tab.ui(ui, ctx, &self.base_dir),
                Tab::Config  => self.config_tab.ui(ui, &self.base_dir),
                Tab::Patches => self.patches_tab.ui(ui, ctx, &self.base_dir),
                Tab::Build   => self.build_tab.ui(ui, ctx, &self.base_dir),
            }
        });
    }
}
```

`base_dir` is resolved at startup: the directory containing the `tkg-gui` binary's
ancestor that contains `submodules/linux-tkg/customization.cfg`.

---

## Tab 1 — Kernel Version Browser (`src/tabs/kernel.rs`)

**Data source:** `https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/refs/tags`

cgit renders tag names as `<a>` links. Parse with `scraper` using selector
`td.name > a` (or `a[href*="/tag/"]`) and filter to `v\d+\.\d+(\.\d+)?`.

**State struct:**
```rust
pub struct KernelTab {
    versions: Vec<String>,              // all fetched tags, sorted newest-first
    filter: String,                     // live search string
    selected: Option<String>,           // currently highlighted version
    fetch_rx: Option<Receiver<FetchResult>>,
    status: String,
}
enum FetchResult { Done(Vec<String>), Error(String) }
```

**Behavior:**
- On "Refresh": spawn `std::thread`, call `kernel_fetcher::fetch_tags()`, send result via channel; call `ctx.request_repaint()`
- Every `update()` call: drain `fetch_rx` to populate `versions`
- `egui::TextEdit` filter box at top
- `egui::ScrollArea` with `ui.selectable_label()` for each matching version
- "Apply to Config" button: calls `config_manager::set_option("_version", selected)`
- Status label: "Fetching…" / "N versions loaded" / error

---

## Tab 2 — Config Options (`src/tabs/config.rs`)

**Config file:** `submodules/linux-tkg/customization.cfg`

**Parser (`src/core/config_manager.rs`):**
- Read file into `Vec<Line>` where `Line` is `Comment(String)` or `Assignment { key, value, raw }`
- Regex: `^(_\w+)\s*=\s*["']?([^"'#\n]*)["']?`
- `get_option(key)` / `set_option(key, value)` mutate the `Vec<Line>` in-place
- `save()` rewrites file preserving comment lines exactly

**State struct:**
```rust
pub struct ConfigTab {
    values: HashMap<String, String>,
    loaded: bool,
    dirty: bool,
    status: String,
}
```

**UI layout:** `egui::ScrollArea` wrapping groups rendered with `ui.group()`.
Options organized into collapsible sections via `egui::CollapsingHeader`:

### CPU Scheduling
| Option | Widget | Values |
|---|---|---|
| `_cpusched` | ComboBox | `""`, `pds`, `bmq`, `bore`, `cfs`, `eevdf`, `upds`, `muqss` |

### Compiler
| Option | Widget | Values |
|---|---|---|
| `_compiler` | ComboBox | `""` (GCC), `llvm` |
| `_compileroptlevel` | ComboBox | `1` (-O2), `2` (-O3), `3` (-Os) |
| `_lto_mode` | ComboBox | `""`, `no`, `full`, `thin` |
| `_llvm_ias` | ComboBox | `0`, `1` |

### Kernel Version & Source
| Option | Widget | Values |
|---|---|---|
| `_version` | TextEdit | free text (synced from Kernel tab) |
| `_git_mirror` | ComboBox | `kernel.org`, `googlesource.com`, `gregkh`, `torvalds` |
| `_distro` | ComboBox | `Arch`, `Ubuntu`, `Debian`, `Fedora`, `Suse`, `Gentoo`, `Generic` |

### CPU & Performance
| Option | Widget | Values |
|---|---|---|
| `_processor_opt` | ComboBox | `""`, `generic`, `zen`, `zen2`, `zen3`, `zen4`, `skylake`, `native_amd`, `native_intel`, `intel` |
| `_timer_freq` | ComboBox | `100`, `250`, `300`, `500`, `750`, `1000` |
| `_tickless` | ComboBox | `0` (periodic), `1` (full), `2` (idle) |
| `_tcp_cong_alg` | ComboBox | `""`, `yeah`, `bbr`, `cubic`, `reno`, `vegas`, `westwood` |
| `_default_cpu_gov` | ComboBox | `""`, `performance`, `ondemand`, `schedutil` |
| `_rqshare` | ComboBox | `none`, `smt`, `mc`, `mc-llc`, `smp`, `all` |

### Configuration Management
| Option | Widget | Values |
|---|---|---|
| `_configfile` | TextEdit | free text / path |
| `_config_updating` | ComboBox | `olddefconfig`, `oldconfig` |
| `_menunconfig` | ComboBox | `false`, `1` (menuconfig), `2` (nconfig), `3` (xconfig) |

### Patches & Features
| Option | Widget | Values |
|---|---|---|
| `_user_patches` | Checkbox | true / false |
| `_community_patches` | TextEdit | space-separated patch names |
| `_clear_patches` | Checkbox | true / false |
| `_openrgb` | Checkbox | true / false |
| `_acs_override` | Checkbox | true / false |
| `_preempt_rt` | Checkbox | true / false |
| `_fsync_backport` | Checkbox | true / false |
| `_ntsync` | Checkbox | true / false |
| `_zenify` | Checkbox | true / false |
| `_glitched_base` | Checkbox | true / false |
| `_bcachefs` | Checkbox | true / false |

### Build & Debug
| Option | Widget | Values |
|---|---|---|
| `_debugdisable` | Checkbox | true / false |
| `_STRIP` | Checkbox | true / false |
| `_ftracedisable` | Checkbox | true / false |
| `_numadisable` | Checkbox | true / false |
| `_misc_adds` | Checkbox | true / false |
| `_kernel_on_diet` | Checkbox | true / false |
| `_modprobeddb` | Checkbox | true / false |
| `_random_trust_cpu` | Checkbox | true / false |
| `_config_fragments` | Checkbox | true / false |
| `_NUKR` | Checkbox | true / false |
| `_force_all_threads` | Checkbox | true / false |

Bottom bar: `[Save Config]` `[Reload]` buttons + dirty indicator.

---

## Tab 3 — Patches (`src/tabs/patches.rs`)

**Directory convention** (linux-tkg):
```
submodules/linux-tkg/linux<MAJOR>.<MINOR>-tkg-userpatches/
e.g.  linux6.13-tkg-userpatches/
```
Files: `*.patch` or `*.mypatch`. Disabled patches: rename to `*.patch.disabled`.

**State struct:**
```rust
pub struct PatchesTab {
    url_input: String,
    filename_input: String,       // auto-filled from URL basename
    kernel_series: String,        // e.g. "6.13", derived from _version
    patches: Vec<PatchEntry>,     // {name, enabled}
    download_rx: Option<Receiver<DownloadResult>>,
    status: String,
}
```

**UI layout:**
- Top group — "Download Patch":
  - URL `TextEdit`
  - Filename `TextEdit` (auto-populated from URL basename on paste)
  - Kernel series `TextEdit` (pre-filled from `_version`)
  - `[Download]` button — spawns thread with `ureq` streaming to file
  - Status / progress label
- Bottom group — "Installed Patches":
  - `ScrollArea` list: each row has `Checkbox` (enable/disable) + filename + `[Delete]`
  - Directory path label
  - `[Open in File Manager]` button (`xdg-open`)

---

## Automatic Userpatch Download & Tracking

This section describes the three new components that extend the basic Patches tab
into a full automatic download and update-tracking system.

---

### Patch Catalog (`src/data/catalog.rs`)

A hardcoded `Vec<CatalogEntry>` of well-known userpatch sources. Each entry
describes a remote patch with a version-aware URL template.

```rust
pub struct CatalogEntry {
    pub id: &'static str,          // unique slug, e.g. "pf-kernel"
    pub name: &'static str,        // human-readable, e.g. "pf-kernel patchset"
    pub description: &'static str, // one-line summary
    pub url_template: &'static str,// URL with {series} placeholder, e.g.
                                   // "https://codeberg.org/pf-kernel/linux/raw/tag/v{series}-pf1/0001-pf-kernel.patch"
    pub filename_template: &'static str, // e.g. "pf-{series}.patch"
    pub supported_series: &'static [&'static str], // e.g. &["6.12", "6.13"]
}

pub fn catalog() -> &'static [CatalogEntry] { &CATALOG }

static CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "pf-kernel",
        name: "pf-kernel patchset",
        description: "Post-factum kernel patchset: BFQ, HRTIMER, graysky2 CPU opts",
        url_template: "https://codeberg.org/pf-kernel/linux/raw/tag/v{series}-pf1/0001-pf-kernel.patch",
        filename_template: "pf-{series}.patch",
        supported_series: &["6.12", "6.13"],
    },
    CatalogEntry {
        id: "clearlinux",
        name: "Clear Linux patches",
        description: "Intel Clear Linux performance and latency patches",
        url_template: "https://raw.githubusercontent.com/clearlinux-pkgs/linux/main/0001-i8042-decrease-debug-message-level-to-info.patch",
        filename_template: "clearlinux-{series}.patch",
        supported_series: &["6.12", "6.13"],
    },
    // … additional entries added as new series are released
];
```

Entries whose `supported_series` does not include the current kernel series are
hidden in the UI. When a new kernel series becomes available, update `supported_series`
and `url_template` accordingly.

---

### Patch Registry (`src/core/patch_registry.rs`)

Persists download metadata across sessions so the UI can show provenance,
age, and staleness of each installed patch.

**Storage location:** `<base_dir>/.tkg-gui/patch_registry.json`
(auto-created on first use; add `.tkg-gui/` to `.gitignore`).

**Data model:**

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct PatchMeta {
    pub filename: String,
    pub kernel_series: String,
    pub source_url: Option<String>,     // None for manually dropped files
    pub catalog_id: Option<String>,     // Some("pf-kernel") if from catalog
    pub sha256: String,                 // hex-encoded SHA-256 of file contents
    pub downloaded_at: String,          // RFC-3339 timestamp, e.g. "2025-06-01T12:00:00Z"
    pub etag: Option<String>,           // last known ETag from server
    pub last_modified: Option<String>,  // last known Last-Modified from server
    pub update_status: UpdateStatus,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum UpdateStatus {
    Unknown,    // never checked
    UpToDate,
    Stale,      // server reported different ETag/Last-Modified
    CheckError(String),
}

#[derive(Serialize, Deserialize, Default)]
pub struct PatchRegistry {
    // key: "<kernel_series>/<filename>", e.g. "6.13/pf-6.13.patch"
    pub patches: HashMap<String, PatchMeta>,
}
```

**API surface:**

```rust
impl PatchRegistry {
    pub fn load(base_dir: &Path) -> Self;         // reads JSON, returns Default on missing
    pub fn save(&self, base_dir: &Path) -> Result<(), String>;
    pub fn record_download(&mut self, meta: PatchMeta);
    pub fn remove(&mut self, series: &str, filename: &str);
    pub fn get(&self, series: &str, filename: &str) -> Option<&PatchMeta>;
    pub fn all_for_series(&self, series: &str) -> Vec<&PatchMeta>;
}
```

**Update checking (`src/core/patch_registry.rs`):**

```rust
/// Sends an HTTP HEAD to the source URL and compares ETag / Last-Modified
/// against the stored values. Runs in a spawned thread.
pub fn check_update(meta: PatchMeta, tx: Sender<UpdateCheckResult>);

pub enum UpdateCheckResult {
    UpToDate { key: String },
    Stale    { key: String },
    Error    { key: String, reason: String },
    NoUrl    { key: String },   // source_url was None
}
```

`check_update` issues `ureq::head(url)` and reads the `ETag` and `Last-Modified`
response headers. If neither value matches what is stored in `PatchMeta`, the
patch is marked `Stale`. A `None` source URL immediately emits `NoUrl`.

---

### SHA-256 Hashing in `patch_manager.rs`

Extend `patch_manager::download_patch` to compute a SHA-256 digest while
streaming the response body to disk:

```rust
use sha2::{Sha256, Digest};

// Inside the streaming loop:
let mut hasher = Sha256::new();
// … read chunks …
hasher.update(&chunk);
// …
let hex = format!("{:x}", hasher.finalize());
// Return alongside DownloadResult::Success
```

The registry record is populated from this result immediately after download.

---

### Enhanced Patches Tab (`src/tabs/patches.rs`)

**Extended state struct:**

```rust
pub struct PatchesTab {
    // --- existing ---
    url_input: String,
    filename_input: String,
    kernel_series: String,
    patches: Vec<PatchEntry>,          // filesystem scan
    download_rx: Option<Receiver<DownloadResult>>,
    status: String,

    // --- new ---
    registry: PatchRegistry,           // loaded at tab construction
    catalog_filter: String,            // search box for catalog entries
    update_rx: Option<Receiver<UpdateCheckResult>>,
    update_status: String,
    show_catalog: bool,                // toggle catalog panel visibility
}
```

**UI layout changes:**

```
┌─────────────────────────────────────────────────────────┐
│  [▼ Available Patches (Catalog)]   kernel series: 6.13  │
│  ┌───────────────────────────────────────────────────┐  │
│  │  🔍 filter catalog…                               │  │
│  │                                                   │  │
│  │  pf-kernel patchset          [Download]           │  │
│  │  Post-factum: BFQ, HRTIMER…  ✓ installed         │  │
│  │                                                   │  │
│  │  Clear Linux patches         [Download]           │  │
│  │  Intel perf & latency…                            │  │
│  └───────────────────────────────────────────────────┘  │
│                                                         │
│  [▼ Download from URL]                                  │
│  URL: ___________________  Filename: __________         │
│  [Download]   status: …                                 │
│                                                         │
│  [▼ Installed Patches]                                  │
│  Dir: submodules/linux-tkg/linux6.13-tkg-userpatches/  │
│  [Open in File Manager]   [Check All for Updates]       │
│  ┌───────────────────────────────────────────────────┐  │
│  │  ☑ pf-6.13.patch          🟢 up-to-date          │  │
│  │    src: codeberg.org/…  2025-06-01  sha: a3f9…   │  │
│  │    [Check Update]  [Re-download]  [Delete]        │  │
│  │                                                   │  │
│  │  ☑ mypatch.patch          ⬜ no source tracked   │  │
│  │    [Delete]                                       │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

**Update status badges:**

| `UpdateStatus` | Badge |
|---|---|
| `Unknown` | ⬜ never checked |
| `UpToDate` | 🟢 up-to-date |
| `Stale` | 🟡 update available |
| `CheckError` | 🔴 check failed |
| no source URL | ⬜ no source tracked |

**Interactions:**

- **Catalog `[Download]`**: Pre-fills URL and filename from catalog template,
  then calls the same download path. On success records `catalog_id` and
  `source_url` in the registry.
- **`[Check Update]`** (per row): spawns `patch_registry::check_update` in a
  thread, result arrives via `update_rx`.
- **`[Check All for Updates]`**: iterates all installed patches that have a
  `source_url`, spawns one thread per patch, all use the same `update_rx`.
- **`[Re-download]`**: re-uses the stored `source_url` and `filename` from the
  registry entry, runs the same download path, updates `sha256` and timestamps.
- **Stale detection on tab open**: if the registry has entries that have never
  been checked (`UpdateStatus::Unknown`) and a `source_url` is present, trigger
  `check_update` automatically in the background.

---

### Auto-check on Kernel Version Change

In `TkgApp::update()`, compare the current `_version` config value against the
previous frame's value. When the kernel series changes:

1. Re-derive `patches_tab.kernel_series` from the new `_version`.
2. Re-scan the patch directory for the new series.
3. Fire `check_update` for all registered patches in the new series that have a
   `source_url`.
4. Show a notification in the Patches tab status bar:
   `"Kernel series changed to 6.14 — checking patches for updates…"`

This gives users an automatic signal to refresh/re-download patches whenever they
switch kernel versions.

---

## Tab 4 — Build (`src/tabs/build.rs`)

**State struct:**
```rust
pub struct BuildTab {
    log: Vec<LogLine>,            // accumulated output
    state: BuildState,            // Idle | Running | Done(i32) | Failed(String)
    rx: Option<Receiver<BuildMsg>>,
    auto_scroll: bool,
}
struct LogLine { text: String, level: LogLevel }
enum LogLevel { Normal, Stage, Warning, Error }
enum BuildMsg { Line(String), Exit(i32), SpawnError(String) }
```

**Subprocess management (`src/core/build_manager.rs`):**
```rust
pub fn start_build(work_dir: PathBuf, tx: Sender<BuildMsg>) {
    std::thread::spawn(move || {
        let child = Command::new("makepkg")
            .arg("-si")
            .current_dir(&work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        // Merge stdout + stderr into combined reader
        // BufReader::lines() -> send each line via tx
        // On exit send BuildMsg::Exit(code)
    });
}
```

**UI layout:**
- Top bar: `[▶ Build]` (green) | `[■ Stop]` (red, enabled while running) | working dir label
- `ScrollArea` with `RichText` lines, colored by `LogLevel`:
  - `Normal` — default
  - `Stage` (lines starting with `==>`) — bold green
  - `Warning` (contains `warning:`) — yellow
  - `Error` (contains `error:`) — red
- Auto-scroll checkbox + "Clear" button
- Bottom bar: state label + exit code

Every frame while `state == Running`: drain `rx`, call `ctx.request_repaint()`.

---

## `src/main.rs`

```rust
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TKG Kernel Builder")
            .with_min_inner_size([900.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "TKG Kernel Builder",
        options,
        Box::new(|_cc| Ok(Box::new(app::TkgApp::new()))),
    )
}
```

---

## Implementation Order

1. Fix `.gitmodules` (add wine-tkg-git, remove duplicate root entry)
2. Add `.tkg-gui/` to `.gitignore`
3. Scaffold `Cargo.toml` (including `serde`, `serde_json`, `sha2`) and empty `src/` module tree
4. Implement `src/core/config_manager.rs`
5. Implement `src/core/kernel_fetcher.rs`
6. Implement `src/data/catalog.rs` — hardcoded `CatalogEntry` list
7. Implement `src/core/patch_registry.rs` — `PatchRegistry` load/save + `check_update`
8. Implement `src/core/patch_manager.rs` — download with SHA-256 streaming hash
9. Implement `src/core/build_manager.rs`
10. Implement `src/app.rs` + `src/main.rs` (wire kernel-version-change detection)
11. Implement `src/tabs/kernel.rs`
12. Implement `src/tabs/config.rs`
13. Implement `src/tabs/patches.rs` — full catalog + registry + update-check UI
14. Implement `src/tabs/build.rs`
15. Commit and push
