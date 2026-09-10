# AGENTS.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

TKG GUI is a Rust desktop application for building custom Linux kernels via the [linux-tkg](https://github.com/Frogging-Family/linux-tkg) build system. It provides a graphical interface for:
- Browsing kernel versions from git.kernel.org
- Editing `customization.cfg` build options
- Managing userpatches (download, enable/disable, update tracking)
- Running `makepkg -si` builds with live log output

## Build & Run Commands

```bash
# Build (debug)
cargo build

# Build (release)
cargo build --release

# Run
cargo run

# Check without building
cargo check

# Format code
cargo fmt

# Run clippy lints
cargo clippy
```

## Submodule Setup

This project requires the linux-tkg submodule. The GUI locates its config at `submodules/linux-tkg/customization.cfg`:

```bash
git submodule update --init --recursive
```

## Architecture

### Technology Choices
- **GUI**: `egui` + `eframe` - immediate-mode rendering, single static binary
- **Async pattern**: `std::thread` + `mpsc` channels (no async runtime)
- **HTTP**: `ureq` (blocking), `scraper` for HTML parsing

### Module Structure

```
src/
├── main.rs          # eframe entry point
├── app.rs           # TkgApp state + tab routing
├── core/            # Business logic (no UI)
│   ├── kernel_fetcher.rs   # Fetches tags from git.kernel.org
│   ├── config_manager.rs   # Parses/writes customization.cfg
│   ├── patch_manager.rs    # Downloads patches, handles compression
│   ├── patch_registry.rs   # Persists patch metadata to .tkg-gui/
│   └── build_manager.rs    # Spawns makepkg, streams output
├── tabs/            # UI components (one per tab)
│   ├── kernel.rs    # Version browser
│   ├── config.rs    # Config editor
│   ├── patches.rs   # Patch management
│   └── build.rs     # Build runner with log display
└── data/
    └── catalog.rs   # Hardcoded patch sources (CatalogEntry)
```

### Key Patterns

**Channel-based async communication**: Background operations (HTTP fetches, subprocess I/O) run in `std::thread` and send results via `mpsc::Sender`. UI drains receivers on each frame via `try_recv()`.

```rust
// Spawning background work
let (tx, rx) = channel();
self.fetch_rx = Some(rx);
thread::spawn(move || {
    let result = do_work();
    let _ = tx.send(result);
    ctx.request_repaint();  // Wake up UI
});

// Draining in update()
if let Some(rx) = &self.fetch_rx {
    if let Ok(result) = rx.try_recv() {
        // Handle result
    }
}
```

**Config file format**: `customization.cfg` uses `_KEY="value"` assignments. The parser preserves comments and line ordering. Keys are prefixed with underscore (e.g., `_cpusched`, `_version`).

**Patch directory convention**: Userpatches go in `linux{series}-tkg-userpatches/` (e.g., `linux6.13-tkg-userpatches/`). Files use `.patch` or `.mypatch` extensions; disabled patches get `.disabled` suffix.

### Local State

The `.tkg-gui/` directory (gitignored) stores:
- `patch_registry.json` - Tracks patch metadata: source URLs, SHA-256 hashes, ETags for update detection

## Adding Catalog Entries

To add new patch sources to the catalog (`src/data/catalog.rs`):

```rust
CatalogEntry {
    id: "unique-id",
    name: "Human-readable name",
    description: "Brief description",
    url_template: "https://example.com/{series}/patch.patch",  // {series} is replaced
    filename_template: "name-{series}.patch",
    supported_series: &["6.12", "6.13"],  // Kernel series this patch supports
},
```


## Automated kernel feature presets (tkg-gui)

Config tab section **Automated Kernel Features (.myfrag)** writes linux-tkg
fragments next to PKGBUILD when saved or when a build starts:

| GUI toggle | Files written | Purpose |
|------------|---------------|---------|
| Xen dom0 / backend | `tkg-gui-xen.myfrag` | Force CONFIG_XEN* dom0/backend stack |
| LVM thin-provisioning | `tkg-gui-lvm-thin.myfrag` | CONFIG_DM_THIN_PROVISIONING + DM deps |
| acpi_call module | `tkg-gui-acpi-call.myfrag`, `tkg-gui-acpi-call-install.sh` | ACPI prereqs + DKMS/source helper |

Enabling any preset sets `_config_fragments=true` and
`_config_fragments_no_confirm=true` so `install.sh`/`makepkg` merge fragments
without interactive prompts.

GUI-only keys stored in `customization.cfg` (ignored by linux-tkg itself):
`_tkg_gui_xen_dom0`, `_tkg_gui_lvm_thin`, `_tkg_gui_acpi_call`.

Note: `_runqueue_sharing` is the correct linux-tkg key (not `_rqshare`).

## Userpatch conflict detection

`src/core/patch_conflicts.rs` compares enabled `*.mypatch` files in
`linux<ver>-tkg-userpatches/` against the bundled patches in
`linux-tkg-patches/<ver>/`, looking for declarations that can only exist once:

| Class | Detected from | Symptom when duplicated |
|-------|---------------|-------------------------|
| Kconfig symbol | `+config NAME` | duplicate symbol, doubled menu entry |
| sysctl procname | `+ .procname = "name"` | duplicate sysctl registration |
| file-scope definition | `+int name =` at **column 0** | redefinition / static-vs-extern |

Column 0 matters: indented declarations are locals and are ignored, which is
what keeps a 199-patch series from producing constant false positives.

Each finding records whether the bundled patch is actually selected by
`customization.cfg` (`Applicability::{Applied, NotApplied, Unknown}`), mirroring
the conditions in `linux-tkg-config/prepare` — e.g. `0013-optimize_harder_O3.patch`
only applies at `_compileroptlevel=2`, `0001-bore.patch` only at
`_cpusched=bore`, and `0001-add-sysctl-to-disallow-...` is replaced by
`0012-linux-hardened.patch` when `_configfile=config_hardened.x86_64` **and**
`_cpusched=cfs`. An unrecognised bundled patch is `Unknown`, never asserted.

Surfaced in two places: the Build tab logs findings before starting a build
(warning only — it never blocks, since a duplicate could be deliberate), and the
Patches tab has a **🔍 Check Conflicts** button to run the scan on demand.

Userpatches apply *after* every bundled patch (`prepare:1825`), so the duplicate
is always the userpatch — and deleting it is the fix.

To validate against a live tree:

```bash
TKG_GUI_CONFLICT_TREE=/path/to/linux-tkg \
  cargo test scan_a_real_tree -- --ignored --nocapture
```

## Config Options Reference

The Config tab edits `submodules/linux-tkg/customization.cfg`. Key options include:
- `_cpusched`: CPU scheduler (pds, bmq, bore, cfs, eevdf)
- `_compiler`: Compiler (gcc or llvm)
- `_lto_mode`: LTO mode (no, full, thin)
- `_version`: Kernel version to build (e.g., "v6.13.1")
- `_processor_opt`: CPU optimization target (zen4, native_amd, etc.)
