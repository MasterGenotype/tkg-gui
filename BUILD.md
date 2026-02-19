# Building TKG GUI

This document covers everything needed to build, run, and develop TKG GUI from source.

## Prerequisites

### Rust Toolchain

Install Rust via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

TKG GUI uses the **stable** toolchain and Rust **2021 edition**. No nightly features are required.

### System Dependencies

The egui/eframe GUI framework requires certain system libraries on Linux. Install them for your distribution:

**Arch Linux / Arch-based:**
```bash
sudo pacman -S libxkbcommon wayland libxcb
```

**Ubuntu / Debian:**
```bash
sudo apt install libxkbcommon-dev libwayland-dev libxcb1-dev
```

**Fedora:**
```bash
sudo dnf install libxkbcommon-devel wayland-devel libxcb-devel
```

### Git Submodules

The linux-tkg build system is included as a git submodule and must be initialized before running the application:

```bash
git submodule update --init --recursive
```

## Building

### Debug Build

Compiles quickly with debug symbols. Recommended during development.

```bash
cargo build
```

Output binary: `target/debug/tkg-gui`

### Release Build

Optimized build for normal use. Significantly smaller and faster than debug.

```bash
cargo build --release
```

Output binary: `target/release/tkg-gui`

## Running

```bash
# Run debug build directly via Cargo
cargo run

# Run release build via Cargo
cargo run --release

# Run a previously compiled release binary
./target/release/tkg-gui
```

The application window opens at a minimum size of 900×700 pixels.

## Development Commands

### Type Checking (fast feedback, no binary produced)

```bash
cargo check
```

### Code Formatting

```bash
cargo fmt
```

To check formatting without applying changes:

```bash
cargo fmt -- --check
```

### Linting

```bash
cargo clippy
```

To treat warnings as errors (matches CI expectations):

```bash
cargo clippy -- -D warnings
```

### Running Tests

```bash
cargo test
```

## Dependency Management

All dependencies are declared in `Cargo.toml` and pinned via `Cargo.lock`. To update dependencies:

```bash
# Update all dependencies within declared version constraints
cargo update

# Update a specific dependency
cargo update -p <crate-name>
```

Key dependencies and their roles:

| Crate | Version | Purpose |
|-------|---------|---------|
| `eframe` | 0.29 | Application window and render loop |
| `egui` | 0.29 | Immediate-mode GUI widgets |
| `ureq` | 2 | Blocking HTTP client for kernel/patch fetching |
| `scraper` | 0.20 | HTML parsing for git.kernel.org version listing |
| `regex` | 1 | Config file parsing |
| `serde` + `serde_json` | 1 | Settings and patch registry serialization |
| `sha2` | 0.10 | SHA-256 hashing for patch integrity checks |
| `xz2` + `flate2` | — | Decompressing `.xz` and `.gz` patch archives |
| `tar` | 0.4 | Extracting tar archives |
| `chrono` | 0.4 | Timestamps in patch registry |

## Project Layout

```
tkg-gui/
├── Cargo.toml           # Package manifest and dependencies
├── Cargo.lock           # Pinned dependency versions
├── src/
│   ├── main.rs          # Entry point — calls eframe::run_native()
│   ├── app.rs           # TkgApp struct, tab enum, update() loop
│   ├── settings.rs      # AppSettings struct and file I/O
│   ├── core/            # Pure business logic, no egui imports
│   │   ├── mod.rs
│   │   ├── kernel_fetcher.rs
│   │   ├── kernel_downloader.rs
│   │   ├── config_manager.rs
│   │   ├── patch_manager.rs
│   │   ├── patch_registry.rs
│   │   ├── build_manager.rs
│   │   └── repo_manager.rs
│   ├── tabs/            # egui panel implementations
│   │   ├── mod.rs
│   │   ├── kernel.rs
│   │   ├── config.rs
│   │   ├── patches.rs
│   │   ├── build.rs
│   │   └── settings.rs
│   └── data/
│       ├── mod.rs
│       └── catalog.rs   # Hardcoded CatalogEntry list
└── submodules/
    └── linux-tkg/       # Frogging-Family/linux-tkg (git submodule)
```

## Architecture Notes

### Concurrency Model

There is no async runtime. Background work (HTTP requests, `makepkg` subprocess I/O) runs in `std::thread::spawn` and sends results back to the UI via `std::sync::mpsc` channels. The egui update loop drains receivers with `try_recv()` on every frame.

### Config File

The Config tab reads and writes `submodules/linux-tkg/customization.cfg` (or the path configured in Settings). The file uses Bash-style `_KEY="value"` assignments. The parser in `src/core/config_manager.rs` preserves comments and line ordering.

### Local State

The `.tkg-gui/` directory at the repo root (gitignored) holds runtime state:
- `patch_registry.json` — patch metadata: URLs, SHA-256 hashes, ETags, timestamps

In installed/release use the equivalent path is `~/.local/share/tkg-gui/`.

## Troubleshooting

**`cargo build` fails with missing system library errors**

Install the system dependencies listed in the [Prerequisites](#prerequisites) section for your distribution.

**Submodule directory is empty or `customization.cfg` is not found**

Run:
```bash
git submodule update --init --recursive
```

**Application fails to open a window (Wayland/X11)**

Ensure `libxkbcommon` and either `libwayland` or `libxcb` are installed. On Wayland compositors, the `WAYLAND_DISPLAY` environment variable must be set.

**`makepkg` not found when starting a build**

TKG GUI requires `makepkg`, which is part of Arch Linux's `pacman` package. The application currently only supports Arch Linux and derivatives.
