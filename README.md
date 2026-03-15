# TKG GUI

A graphical interface for building custom Linux kernels using the [linux-tkg](https://github.com/Frogging-Family/linux-tkg) build system.

## Overview

TKG GUI is a Rust desktop application that wraps the linux-tkg kernel build system in an easy-to-use interface. Instead of manually editing configuration files and running build commands, you can browse kernel versions, view changelogs, download sources, tweak build options, manage patches, and launch builds — all from a single window.

### Features

- **Kernel Browser** — Browse kernel versions fetched from git.kernel.org with release dates; view the commit shortlog between any two releases; download kernel sources directly from cdn.kernel.org with a live progress bar
- **Configuration Editor** — Edit `customization.cfg` build options through grouped UI widgets (CPU scheduler, compiler, LTO mode, processor optimizations, distro, and more); unsaved changes are highlighted
- **Patch Management** — Built-in catalog of curated patches (one-click download); download patches from any URL; enable/disable individual patches; SHA-256 integrity tracking; ETag/Last-Modified update checking
- **Build Runner** — Execute `makepkg -si` (Arch) or `./install.sh install` (other distros) with live streaming log output, colour-coded by severity, and an interactive input field for responding to build prompts
- **Settings** — Configure the linux-tkg repository path; clone linux-tkg from GitHub directly within the app; install the tkg-gui binary to `~/.local/bin`

## Requirements

- **Rust** (stable toolchain) — install via [rustup](https://rustup.rs/)
- **linux-tkg dependencies** — see the [linux-tkg README](https://github.com/Frogging-Family/linux-tkg) for the full list of required packages
- **Arch Linux / derivatives**: `makepkg` (part of `pacman`) for the build step
- **Other distros** (Ubuntu, Debian, Fedora, Suse, Gentoo): the linux-tkg `install.sh` script is used instead; the `_distro` setting in Config controls which build command is used

> The GUI itself runs on any Linux desktop that has `libxkbcommon` and either Wayland or X11/XCB libraries available. See [BUILD.md](BUILD.md) for distro-specific package names.

## Quick Start

```bash
# Clone the repository
git clone https://github.com/MasterGenotype/tkg-gui.git
cd tkg-gui

# Build and run
cargo run --release
```

On first launch, go to the **Settings** tab to either:
- **Clone linux-tkg automatically** — click "Clone linux-tkg" to fetch it to the default data directory (`~/.local/share/tkg-gui/linux-tkg`), or
- **Point to an existing checkout** — enter the path and click "Save Path"

If you cloned the repo with submodules (`--recurse-submodules`) and want to use the bundled submodule instead, set the linux-tkg path to `<repo>/submodules/linux-tkg`.

## Usage

1. **Kernel tab** — Click **Refresh** to load the version list. Select a version to see its release date and commit shortlog. Click **Download Kernel Sources** to fetch the `.tar.xz` from kernel.org.
2. **Config tab** — Adjust build options. When you have selected a kernel version, click **📋 Apply Version to Config** in the top toolbar to write the version into `customization.cfg` automatically.
3. **Patches tab** — Browse the built-in patch catalog filtered to your kernel series; click **Download** to install any patch. Use "Download from URL" for custom patches. Manage installed patches (enable/disable, check for updates, re-download, delete).
4. **Build tab** — Click **▶ Build** to start the build. Logs stream in real time with colour coding. Use the **Input** field at the bottom to respond to interactive prompts (e.g. confirmation questions from `makepkg`).
5. **Settings tab** — Set the path to your linux-tkg checkout, clone it from GitHub, or install the tkg-gui binary to `~/.local/bin`.

### Application Paths

| Purpose | Default Path |
|---------|-------------|
| Configuration | `~/.config/tkg-gui/settings.json` |
| Application data | `~/.local/share/tkg-gui/` |
| linux-tkg checkout | `~/.local/share/tkg-gui/linux-tkg` |
| Patch registry | `~/.local/share/tkg-gui/patch_registry.json` |
| Downloaded kernel sources | `~/.cache/tkg-gui/kernel-sources/` |

### Built-in Patch Catalog

The Patches tab includes a curated catalog of commonly used patches:

| Patch | Description |
|-------|-------------|
| ACS Override | Split IOMMU groups for VFIO passthrough |
| BBRv3 TCP | Google's BBRv3 TCP congestion control |
| CachyOS Kernel Fixes | Kernel fixes from the CachyOS project |
| Graysky CPU Optimizations | Additional compiler optimizations for specific CPU microarchitectures |
| Futex2/waitv Backport | Steam/Proton compatibility futex2 backport |
| ZSTD Upstream Updates | Latest upstream ZSTD compression improvements |
| AMD P-State Improvements | Enhanced AMD P-State driver patches |
| le9 OOM Protection | Protect the working set under memory pressure |

Catalog patches are filtered by kernel series so only compatible patches are shown.

## Installation (Arch Linux)

An AUR-style `PKGBUILD` is provided in `pkg/`:

```bash
cd pkg
makepkg -si
```

This installs:
- `/usr/bin/tkg-gui` — the application binary
- `/usr/share/tkg-gui/linux-tkg` — bundled linux-tkg checkout
- `/usr/share/applications/tkg-gui.desktop` — desktop entry (launches via `pkexec`)
- `/usr/share/polkit-1/actions/com.tkg-gui.policy` — PolicyKit policy for privilege elevation

## Project Structure

```
tkg-gui/
├── src/
│   ├── main.rs              # Application entry point
│   ├── app.rs               # Main app state, tab routing, toolbar
│   ├── settings.rs          # User settings and file paths
│   ├── core/                # Business logic (no UI code)
│   │   ├── http_client.rs       # Shared HTTP agent wrapper
│   │   ├── kernel_fetcher.rs    # Fetches version list and commit shortlog from git.kernel.org
│   │   ├── kernel_downloader.rs # Downloads and extracts kernel source tarballs
│   │   ├── config_manager.rs    # Parses and writes customization.cfg
│   │   ├── patch_manager.rs     # Downloads and manages patches
│   │   ├── patch_registry.rs    # Persists patch metadata (SHA-256, ETags, timestamps)
│   │   ├── build_manager.rs     # Runs makepkg/install.sh and streams output
│   │   ├── repo_manager.rs      # Clones linux-tkg repository
│   │   └── work_dir.rs         # Temporary working directory management
│   ├── tabs/                # UI panels (one per tab)
│   │   ├── kernel.rs        # Kernel browser with shortlog and source download
│   │   ├── config.rs        # Configuration editor
│   │   ├── patches.rs       # Patch catalog, download, and management
│   │   ├── build.rs         # Build runner with log display and interactive input
│   │   └── settings.rs      # Settings panel with clone and install helpers
│   └── data/
│       └── catalog.rs       # Built-in patch source catalog
├── pkg/
│   └── PKGBUILD             # Arch Linux package build script
├── tkg-gui.desktop          # Desktop entry file
├── com.tkg-gui.policy       # PolicyKit authorization policy
└── submodules/
    └── linux-tkg/           # linux-tkg build system (git submodule)
```

## Technology Stack

| Component | Library |
|-----------|---------|
| GUI framework | [egui](https://github.com/emilk/egui) + [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) 0.29 |
| HTTP client | [ureq](https://github.com/algesten/ureq) 2 |
| HTML parsing | [scraper](https://github.com/causal-agent/scraper) 0.20 |
| Config file parsing | [regex](https://github.com/rust-lang/regex) 1 |
| Serialization | [serde](https://serde.rs/) + serde_json |
| Timestamps | [chrono](https://github.com/chronotope/chrono) 0.4 |
| Hashing | [sha2](https://github.com/RustCrypto/hashes) 0.10 |
| Compression | [xz2](https://github.com/alexcrichton/xz2-rs) 0.1, [flate2](https://github.com/rust-lang/flate2-rs) 1 |
| Archives | [tar](https://github.com/alexcrichton/tar-rs) 0.4 |
| TLS | [native-tls](https://github.com/sfackler/rust-native-tls) 0.2 |

Background work (HTTP requests, subprocess I/O) runs in `std::thread` and communicates with the UI via `mpsc` channels — no async runtime is used.

## Contributing

See [AGENTS.md](AGENTS.md) for architecture notes, coding patterns, and guidance on adding new patch catalog entries.

## License

This project is open source. See [LICENSE](LICENSE) for details.
