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
├── src/
│   ├── main.rs                # eframe::run_native() entry point
│   ├── app.rs                 # TkgApp struct + eframe::App impl
│   ├── tabs/
│   │   ├── mod.rs
│   │   ├── kernel.rs          # Kernel version browser tab
│   │   ├── config.rs          # Config options tab
│   │   ├── patches.rs         # Patch download/manage tab
│   │   └── build.rs           # makepkg runner + log tab
│   └── core/
│       ├── mod.rs
│       ├── kernel_fetcher.rs  # HTTP fetch + HTML parse git.kernel.org tags
│       ├── config_manager.rs  # Parse/write customization.cfg
│       ├── patch_manager.rs   # Download patches, manage patch dirs
│       └── build_manager.rs   # Subprocess spawn + line-by-line channel output
└── submodules/
    ├── linux-tkg/              # git submodule (Frogging-Family/linux-tkg)
    └── wine-tkg-git/           # git submodule (Frogging-Family/wine-tkg-git)
```

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
2. Scaffold `Cargo.toml` and empty `src/` module tree
3. Implement `src/core/config_manager.rs`
4. Implement `src/core/kernel_fetcher.rs`
5. Implement `src/core/patch_manager.rs`
6. Implement `src/core/build_manager.rs`
7. Implement `src/app.rs` + `src/main.rs`
8. Implement `src/tabs/kernel.rs`
9. Implement `src/tabs/config.rs`
10. Implement `src/tabs/patches.rs`
11. Implement `src/tabs/build.rs`
12. Commit and push to `claude/plan-kernel-gui-M25jK`
