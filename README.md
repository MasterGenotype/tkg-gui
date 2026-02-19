# TKG GUI

A graphical interface for building custom Linux kernels using the [linux-tkg](https://github.com/Frogging-Family/linux-tkg) build system.

## Overview

TKG GUI is a Rust desktop application that wraps the linux-tkg kernel build system in an easy-to-use interface. Instead of manually editing configuration files and running build commands, you can browse kernel versions, tweak build options, manage patches, and launch builds — all from a single window.

### Features

- **Kernel Browser** — Browse and select kernel versions fetched from git.kernel.org
- **Configuration Editor** — Edit all `customization.cfg` build options through UI widgets
- **Patch Management** — Download, enable/disable, and track updates for kernel patches
- **Build Runner** — Execute `makepkg -si` builds with live streaming log output
- **Settings** — Configure the linux-tkg repository path and other preferences

## Requirements

- **Rust** (stable toolchain) — install via [rustup](https://rustup.rs/)
- **makepkg** — part of the `pacman` package manager (Arch Linux / Arch-based distros)
- **linux-tkg dependencies** — see the [linux-tkg README](https://github.com/Frogging-Family/linux-tkg) for the full list of required packages

> TKG GUI currently targets Arch Linux and derivatives, as the underlying linux-tkg build system uses `makepkg`.

## Quick Start

```bash
# Clone the repository with submodules
git clone --recurse-submodules https://github.com/MasterGenotype/tkg-gui.git
cd tkg-gui

# Build and run
cargo run --release
```

If you already cloned without `--recurse-submodules`, initialize submodules manually:

```bash
git submodule update --init --recursive
```

## Usage

1. **Kernel tab** — Select the kernel version you want to build.
2. **Config tab** — Adjust build options (CPU scheduler, compiler, LTO mode, processor optimizations, etc.).
3. **Patches tab** — Browse the patch catalog and download any patches you want applied.
4. **Build tab** — Start the build. Logs stream in real time.
5. **Settings tab** — Set the path to your linux-tkg checkout if it differs from the default.

### Application Paths

| Purpose | Default Path |
|---------|-------------|
| Configuration | `~/.config/tkg-gui/settings.json` |
| Application data | `~/.local/share/tkg-gui/` |
| Patch registry | `~/.local/share/tkg-gui/patch_registry.json` |

## Project Structure

```
tkg-gui/
├── src/
│   ├── main.rs          # Application entry point
│   ├── app.rs           # Main app state and tab routing
│   ├── settings.rs      # User settings and file paths
│   ├── core/            # Business logic (no UI code)
│   │   ├── kernel_fetcher.rs    # Fetches kernel versions from git.kernel.org
│   │   ├── kernel_downloader.rs # Downloads kernel source archives
│   │   ├── config_manager.rs    # Parses and writes customization.cfg
│   │   ├── patch_manager.rs     # Downloads and manages patches
│   │   ├── patch_registry.rs    # Persists patch metadata
│   │   ├── build_manager.rs     # Runs makepkg and streams output
│   │   └── repo_manager.rs      # Repository management utilities
│   ├── tabs/            # UI panels (one per tab)
│   │   ├── kernel.rs    # Kernel version browser
│   │   ├── config.rs    # Configuration editor
│   │   ├── patches.rs   # Patch management
│   │   ├── build.rs     # Build runner with log display
│   │   └── settings.rs  # Settings panel
│   └── data/
│       └── catalog.rs   # Built-in patch source catalog
└── submodules/
    └── linux-tkg/       # linux-tkg build system (git submodule)
```

## Technology Stack

| Component | Library |
|-----------|---------|
| GUI framework | [egui](https://github.com/emilk/egui) + [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) 0.29 |
| HTTP client | [ureq](https://github.com/algesten/ureq) 2 |
| HTML parsing | [scraper](https://github.com/causal-agent/scraper) 0.20 |
| Serialization | [serde](https://serde.rs/) + serde_json |
| Hashing | sha2 |
| Compression | xz2, flate2 |
| Archives | tar |

Background work (HTTP requests, subprocess I/O) runs in `std::thread` and communicates with the UI via `mpsc` channels — no async runtime is used.

## Contributing

See [AGENTS.md](AGENTS.md) for architecture notes, coding patterns, and guidance on adding new patch catalog entries.

## License

This project is open source. See [LICENSE](LICENSE) for details.
