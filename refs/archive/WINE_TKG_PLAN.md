# Phase 2: Wine-TKG-Git Integration Plan

## Overview

Add wine-tkg-git as a second installation phase to TKG GUI, following the same
architecture patterns established for linux-tkg in Phase 1. Users will be able
to configure, build, and install custom Wine packages from
https://github.com/Frogging-Family/wine-tkg-git using the same egui-based
interface they use for kernel builds.

**Phase 1 (existing):** linux-tkg kernel builder — Kernel, Config, Patches, Build, Settings tabs.
**Phase 2 (new):** wine-tkg-git Wine builder — new Wine tab group integrated into the same app.

---

## Upstream Repo Structure

wine-tkg-git nests its buildable package inside a subdirectory:

```
wine-tkg-git/               ← git clone root (wine_tkg_path)
└── wine-tkg-git/           ← PKGBUILD lives here (build working dir)
    ├── PKGBUILD
    ├── customization.cfg   ← wine build options
    ├── wine-tkg-patches/   ← bundled patch files
    └── ...
```

Key consequence: the build working directory is `<wine_tkg_path>/wine-tkg-git/`,
not the repo root. Config file is `<wine_tkg_path>/wine-tkg-git/customization.cfg`.

---

## Files to Add / Modify

| File | Change |
|---|---|
| `.gitmodules` | Remove duplicate `wine-tkg-wit` entry |
| `src/settings.rs` | Add `wine_tkg_path` field with serde default |
| `src/core/mod.rs` | Export `wine_config_manager` and `wine_build_manager` |
| `src/core/wine_config_manager.rs` | **New** — parse/write wine-tkg's `customization.cfg` |
| `src/core/wine_build_manager.rs` | **New** — spawn `makepkg -si` in the wine build dir |
| `src/tabs/mod.rs` | Export `wine` module |
| `src/tabs/wine.rs` | **New** — Wine tab UI (Setup + Config + Build sections) |
| `src/tabs/settings.rs` | Add wine-tkg path field + clone section |
| `src/app.rs` | Add `Tab::Wine`, `wine_tab` field, route in `update()` |

---

## Step 1 — Submodule Cleanup

Remove the accidental duplicate from `.gitmodules`. The corrected file:

```ini
[submodule "submodules/linux-tkg"]
    path = submodules/linux-tkg
    url = https://github.com/Frogging-Family/linux-tkg

[submodule "submodules/wine-tkg-git"]
    path = submodules/wine-tkg-git
    url = https://github.com/Frogging-Family/wine-tkg-git
```

Remove the stale `submodules/wine-tkg-wit` entry and its directory:

```bash
git rm --cached submodules/wine-tkg-wit
rm -rf submodules/wine-tkg-wit
```

---

## Step 2 — Settings (`src/settings.rs`)

Add `wine_tkg_path` alongside `linux_tkg_path`:

```rust
fn default_wine_tkg_path() -> PathBuf {
    home_dir()
        .join(".local")
        .join("share")
        .join("tkg-gui")
        .join("wine-tkg-git")
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AppSettings {
    #[serde(default = "default_linux_tkg_path")]
    pub linux_tkg_path: PathBuf,

    #[serde(default = "default_wine_tkg_path")]
    pub wine_tkg_path: PathBuf,
}
```

Add a detection helper parallel to `is_cloned()`:

```rust
/// Returns true if wine-tkg-git appears to be cloned at wine_tkg_path.
/// Detects by checking for the inner customization.cfg.
pub fn is_wine_cloned(&self) -> bool {
    self.wine_tkg_path
        .join("wine-tkg-git")
        .join("customization.cfg")
        .exists()
}
```

---

## Step 3 — Wine Config Manager (`src/core/wine_config_manager.rs`)

Reuse the same bash-style key=value format as `config_manager.rs`. The API
surface is identical — only the config file path changes.

```rust
/// Path: <wine_tkg_path>/wine-tkg-git/customization.cfg
pub fn wine_config_path(wine_tkg_path: &Path) -> PathBuf {
    wine_tkg_path.join("wine-tkg-git").join("customization.cfg")
}

pub fn load_wine_config(wine_tkg_path: &Path) -> Result<ConfigMap, String> { ... }
pub fn save_wine_config(wine_tkg_path: &Path, map: &ConfigMap) -> Result<(), String> { ... }
pub fn get_option(map: &ConfigMap, key: &str) -> Option<String> { ... }
pub fn set_option(map: &mut ConfigMap, key: &str, value: &str) { ... }
```

Internally, delegate to the existing `config_manager` parsing primitives
(the `Line` enum and regex). No duplication of parsing logic is needed — just
expose a parallel entry point with the different file path.

---

## Step 4 — Wine Build Manager (`src/core/wine_build_manager.rs`)

Mirrors `build_manager.rs` exactly. The only difference is the working
directory: `<wine_tkg_path>/wine-tkg-git/` (the inner subdirectory).

```rust
pub enum WineBuildMsg {
    Line(String),
    Exit(i32),
    SpawnError(String),
}

/// Runs `makepkg -si` in `<wine_tkg_path>/wine-tkg-git/`.
pub fn start_wine_build(wine_tkg_path: PathBuf, tx: Sender<WineBuildMsg>) {
    thread::spawn(move || {
        let work_dir = wine_tkg_path.join("wine-tkg-git");
        let result = Command::new("makepkg")
            .arg("-si")
            .current_dir(&work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        // ... same stdout/stderr merging pattern as build_manager.rs ...
    });
}
```

Add a `clone_wine_tkg()` function to `repo_manager.rs` (or a new
`wine_repo_manager.rs`) following the same pattern as `clone_linux_tkg()`:

```rust
/// Clone https://github.com/Frogging-Family/wine-tkg-git into `dest`.
pub fn clone_wine_tkg(dest: PathBuf, tx: Sender<CloneMsg>) {
    // identical structure to clone_linux_tkg() with different URL
}
```

`CloneMsg` is already generic enough — reuse it from `repo_manager.rs`.

---

## Step 5 — Wine Tab (`src/tabs/wine.rs`)

A single tab with three collapsible sections.

### State Struct

```rust
pub struct WineTab {
    // Setup section
    clone_log: Vec<String>,
    clone_rx: Option<Receiver<CloneMsg>>,
    clone_running: bool,
    clone_status: String,

    // Config section
    config: HashMap<String, String>,
    config_loaded: bool,
    config_dirty: bool,
    config_status: String,

    // Build section
    log: Vec<String>,
    build_rx: Option<Receiver<WineBuildMsg>>,
    build_running: bool,
    build_status: String,
    auto_scroll: bool,
}
```

### UI Layout

```
Wine Builder
════════════

[▼ Setup]
  Path to wine-tkg-git clone:
  [ /home/user/.local/share/tkg-gui/wine-tkg-git        ]
  ✓ wine-tkg-git found        [Clone wine-tkg-git]  (spinner)
  ─── clone log (scrollable, 120px max) ───────────────────

[▼ Configuration]
  (disabled / grayed out when not yet cloned)

  ┌─ Wine Source ──────────────────────────────────────────┐
  │  Version/tag:  [ _____________ ]  (free text)          │
  │  Commit:       [ _____________ ]  (free text, optional) │
  └────────────────────────────────────────────────────────┘

  ┌─ Sync & Patches ───────────────────────────────────────┐
  │  [x] Wine-Staging (_use_staging)                       │
  │  [x] Esync        (_esync)                             │
  │  [x] Fsync        (_fsync)                             │
  │  [x] Ntsync       (_ntsync)                            │
  │  [x] Proton patches (_protonify)                       │
  │  [x] Game Drive   (_game_drive)                        │
  └────────────────────────────────────────────────────────┘

  ┌─ Compiler ─────────────────────────────────────────────┐
  │  Compiler:   [gcc ▼]    (gcc, clang)                   │
  │  [x] O3 optimisation (_O3)                             │
  │  [x] LTO (_lto)                                        │
  └────────────────────────────────────────────────────────┘

  ┌─ Wine Modules ─────────────────────────────────────────┐
  │  [x] WoW64 (disable with _no_wow64)                    │
  └────────────────────────────────────────────────────────┘

  [Save Config]  [Reload]  ●dirty

[▼ Build]
  (disabled when not cloned)
  [▶ Build Wine]   working dir: .../wine-tkg-git/wine-tkg-git/
  [x] Auto-scroll   [Clear Log]

  ┌─ log ───────────────────────────────────────────────────
  │ ==> Entering fakeroot environment...
  │ ==> Starting build()...
  │ ...
  └────────────────────────────────────────────────────────
```

### wine-tkg Config Options to Expose

| Key | Widget | Values |
|---|---|---|
| `_wine_version` | TextEdit | e.g. `"8.21"`, `"9.0"` |
| `_wine_commit` | TextEdit | git SHA or empty |
| `_use_staging` | Checkbox | true / false |
| `_esync` | Checkbox | true / false |
| `_fsync` | Checkbox | true / false |
| `_ntsync` | Checkbox | true / false |
| `_protonify` | Checkbox | true / false |
| `_game_drive` | Checkbox | true / false |
| `_compiler` | ComboBox | `""` (GCC), `clang` |
| `_O3` | Checkbox | true / false |
| `_lto` | Checkbox | true / false |
| `_no_wow64` | Checkbox (inverted label "Enable WoW64") | true / false |

---

## Step 6 — App Integration (`src/app.rs`)

Add `Tab::Wine` variant and a `wine_tab` field:

```rust
#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    Kernel,
    Config,
    Patches,
    Build,
    Settings,
    Wine,        // ← new
}

pub struct TkgApp {
    // ... existing fields ...
    wine_tab: WineTab,   // ← new
}
```

In `TkgApp::new()`:
```rust
wine_tab: WineTab::default(),
```

In `update()`, add the tab button and route:
```rust
ui.selectable_value(&mut self.active_tab, Tab::Wine, "🍷 Wine");

// In the CentralPanel match:
Tab::Wine => self.wine_tab.ui(ui, ctx, &mut self.settings),
```

The Wine tab receives `&mut AppSettings` so it can read/write `wine_tkg_path`
inline (same pattern as SettingsTab).

---

## Step 7 — Settings Tab Extension (`src/tabs/settings.rs`)

Add a "wine-tkg Repository Path" collapsing section after the linux-tkg section.
It mirrors the linux-tkg section exactly: path text field, Save Path button,
clone status indicator, Clone button, and scrollable clone log.

Add corresponding state fields to `SettingsTab`:

```rust
pub struct SettingsTab {
    // ... existing fields ...

    // Wine clone state
    wine_path_input: String,
    wine_clone_log: Vec<String>,
    wine_clone_rx: Option<Receiver<CloneMsg>>,
    wine_clone_running: bool,
    wine_clone_status: String,
}
```

The wine path section in `Settings` is optional — since the Wine tab already
contains its own path field and clone button, the Settings tab section is purely
for discoverability. Decide during implementation whether to keep it in Settings
or only in the Wine tab to avoid duplication.

**Recommendation:** Keep path + clone in the Wine tab only. The Settings tab's
"App Directories" section should display `wine_tkg_path` for reference.

---

## Implementation Order

1. Fix `.gitmodules` — remove `wine-tkg-wit`, verify `wine-tkg-git` entry is correct.
2. Add `wine_tkg_path` to `src/settings.rs` + `is_wine_cloned()`.
3. Add `clone_wine_tkg()` to `src/core/repo_manager.rs`.
4. Create `src/core/wine_config_manager.rs` delegating to existing parsing utilities.
5. Create `src/core/wine_build_manager.rs` mirroring `build_manager.rs`.
6. Update `src/core/mod.rs` to export the two new modules.
7. Create `src/tabs/wine.rs` with `WineTab` struct and `ui()` method.
8. Update `src/tabs/mod.rs` to export `wine`.
9. Update `src/app.rs` to add `Tab::Wine` + `wine_tab` + routing.
10. Update `src/tabs/settings.rs` — add `wine_tkg_path` to the App Directories display.
11. Compile, fix type errors, test manually.
12. Commit and push.

---

## Non-Goals (Out of Scope for Phase 2)

- proton-tkg builds (separate PKGBUILD in the same repo; can be Phase 3).
- Patch management for wine (wine-tkg-git patches are self-contained in the repo).
- Automatic wine version fetching (no equivalent of the kernel version browser;
  users set the version manually in customization.cfg).
- Cross-distro support (wine-tkg also targets Arch/makepkg like linux-tkg).

---

## Notes on Config Re-use

`config_manager.rs` already provides a generic bash-style config parser.
`wine_config_manager.rs` should call the same internal functions, not
duplicate them. If needed, refactor `config_manager.rs` to expose a
`load_config(path: &Path)` and `save_config(path: &Path, map: &ConfigMap)`
that both managers call with their respective paths. This avoids any code
duplication and makes both configs use identical parsing semantics.
