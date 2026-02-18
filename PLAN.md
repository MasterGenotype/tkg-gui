# TKG GUI — Implementation Plan

## Overview

A Python/PyQt6 desktop GUI for building custom Linux kernels via linux-tkg. Four main tabs:
1. **Kernel** — Browse/search kernel versions from git.kernel.org
2. **Config** — Edit all `customization.cfg` options via dropdowns/checkboxes
3. **Patches** — Download and manage custom user patches
4. **Build** — Run `makepkg` and stream live log output

---

## Repository Layout

```
tkg-gui/
├── main.py                        # Entry point
├── requirements.txt               # PyQt6, requests
├── gui/
│   ├── __init__.py
│   ├── main_window.py             # QMainWindow + QTabWidget
│   ├── kernel_tab.py              # Kernel version browser tab
│   ├── config_tab.py              # Config options tab (dropdowns)
│   ├── patches_tab.py             # Patch download/manage tab
│   └── build_tab.py               # makepkg runner + log output tab
├── core/
│   ├── __init__.py
│   ├── kernel_fetcher.py          # HTTP fetch & parse git.kernel.org tags
│   ├── config_manager.py          # Parse/write customization.cfg
│   ├── patch_manager.py           # Download patches, manage patch dir
│   └── build_manager.py           # QProcess wrapper for makepkg
└── submodules/
    ├── linux-tkg/                  # git submodule (Frogging-Family/linux-tkg)
    └── wine-tkg-git/               # git submodule (Frogging-Family/wine-tkg-git)
```

---

## Submodules

Update `.gitmodules` to add `wine-tkg-git`:

```ini
[submodule "submodules/linux-tkg"]
    path = submodules/linux-tkg
    url = https://github.com/Frogging-Family/linux-tkg

[submodule "submodules/wine-tkg-git"]
    path = submodules/wine-tkg-git
    url = https://github.com/Frogging-Family/wine-tkg-git
```

Remove the duplicate `linux-tkg` root-level submodule entry.

---

## Technology Stack

| Component | Library |
|---|---|
| GUI framework | PyQt6 |
| HTTP requests | requests |
| HTML parsing | html.parser (stdlib) |
| Subprocess | QProcess (non-blocking) |
| Config parsing | Custom regex-based parser |

---

## Tab 1 — Kernel Version Browser (`kernel_tab.py`)

**Data source:** `https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/refs/tags`

The cgit HTML page lists all tags. Parse with `html.parser` to extract `v6.x.y` tags.

**UI elements:**
- Search/filter `QLineEdit` at top
- `QListWidget` showing all matching kernel version tags (sorted newest-first)
- "Refresh" `QPushButton` (fetches tags in a `QThread` worker)
- "Apply Version" `QPushButton` — writes `_version` into `customization.cfg`
- Status label showing currently selected version

**Kernel fetcher logic (`core/kernel_fetcher.py`):**
```python
# Fetch tags page, parse <a> hrefs under "refs/tags" cgit table
# Filter to tags matching pattern: v\d+\.\d+(\.\d+)?
# Return sorted list newest-first using version tuple key
```

---

## Tab 2 — Config Options (`config_tab.py`)

**Config file:** `submodules/linux-tkg/customization.cfg`

**Parser (`core/config_manager.py`):**
- Read the file line by line
- Preserve comments and blank lines
- Extract `_option="value"` or `_option=value` assignments
- On save: replace only the assignment lines, preserve everything else

**UI layout:**
- `QScrollArea` containing a `QFormLayout`
- Each option rendered as a labeled row with the appropriate widget:

| Option | Widget Type | Values |
|---|---|---|
| `_cpusched` | QComboBox | `pds`, `bmq`, `bore`, `cfs`, `eevdf`, `upds`, `muqss` |
| `_compiler` | QComboBox | `` (GCC/empty), `llvm` |
| `_compileroptlevel` | QComboBox | `1` (-O2), `2` (-O3), `3` (-Os) |
| `_lto_mode` | QComboBox | `no`, `full`, `thin` |
| `_timer_freq` | QComboBox | `100`, `250`, `300`, `500`, `750`, `1000` |
| `_tickless` | QComboBox | `0` (periodic), `1` (full), `2` (idle) |
| `_processor_opt` | QComboBox | `generic`, `zen`, `zen2`, `zen3`, `skylake`, `native_amd`, `native_intel`, `intel` (+ others) |
| `_tcp_cong_alg` | QComboBox | `yeah`, `bbr`, `cubic`, `reno`, `vegas`, `westwood` |
| `_default_cpu_gov` | QComboBox | `performance`, `ondemand`, `schedutil` |
| `_rqshare` | QComboBox | `none`, `smt`, `mc`, `mc-llc`, `smp`, `all` |
| `_distro` | QComboBox | `Arch`, `Ubuntu`, `Debian`, `Fedora`, `Suse`, `Gentoo`, `Generic` |
| `_config_updating` | QComboBox | `olddefconfig`, `oldconfig` |
| `_menunconfig` | QComboBox | `false`, `1` (menuconfig), `2` (nconfig), `3` (xconfig) |
| `_git_mirror` | QComboBox | `kernel.org`, `googlesource.com`, `gregkh`, `torvalds` |
| `_version` | QLineEdit | Free text (set from Kernel tab) |
| `_configfile` | QLineEdit | Free text / path |
| `_user_patches` | QCheckBox | true/false |
| `_community_patches` | QLineEdit | Space-separated patch names |
| `_clear_patches` | QCheckBox | true/false |
| `_openrgb` | QCheckBox | true/false |
| `_acs_override` | QCheckBox | true/false |
| `_preempt_rt` | QCheckBox | true/false |
| `_fsync_backport` | QCheckBox | true/false |
| `_ntsync` | QCheckBox | true/false |
| `_zenify` | QCheckBox | true/false |
| `_debugdisable` | QCheckBox | true/false |
| `_STRIP` | QCheckBox | true/false |
| `_ftracedisable` | QCheckBox | true/false |
| `_numadisable` | QCheckBox | true/false |
| `_misc_adds` | QCheckBox | true/false |
| `_kernel_on_diet` | QCheckBox | true/false |
| `_modprobeddb` | QCheckBox | true/false |
| `_random_trust_cpu` | QCheckBox | true/false |
| `_glitched_base` | QCheckBox | true/false |
| `_bcachefs` | QCheckBox | true/false |
| `_NUKR` | QCheckBox | true/false |
| `_force_all_threads` | QCheckBox | true/false |
| `_config_fragments` | QCheckBox | true/false |
| `_llvm_ias` | QComboBox | `0`, `1` |

- "Save Config" `QPushButton` at bottom — writes modified values back to file
- "Reload" `QPushButton` — re-reads from file

---

## Tab 3 — Patches (`patches_tab.py`)

**Directory convention** (from linux-tkg):
- Patch directory: `submodules/linux-tkg/linux<MAJOR>.<MINOR>-tkg-userpatches/`
- e.g. for kernel 6.13: `linux6.13-tkg-userpatches/`
- Individual patches: `*.patch` or `*.mypatch` files in that directory

**UI elements:**
- **Top section — Download patch:**
  - `QLineEdit` for patch URL
  - `QLineEdit` for filename override (auto-filled from URL basename)
  - `QComboBox` for target kernel series (e.g. `6.13`, `6.12`) — populated from `_version`
  - "Download" `QPushButton` — fetches URL and saves to correct directory
  - Progress/status label

- **Bottom section — Manage patches:**
  - `QListWidget` with checkboxes showing `.patch`/`.mypatch` files in the selected patch dir
  - Checked = included (file present), unchecked = excluded (file renamed `.disabled`)
  - "Delete" button for selected patch
  - "Open Directory" button — opens file manager to patch dir
  - Directory path label

**Patch manager logic (`core/patch_manager.py`):**
```python
def get_patch_dir(version: str, base_dir: str) -> Path:
    # Extract major.minor from version string (strip leading 'v', trailing patch level)
    # Returns base_dir / f"linux{major}.{minor}-tkg-userpatches"

def download_patch(url: str, target_path: Path) -> None:
    # requests.get(url, stream=True) -> write to target_path

def list_patches(patch_dir: Path) -> list[PatchFile]:
    # Returns list of .patch and .mypatch files with enabled/disabled status
```

---

## Tab 4 — Build (`build_tab.py`)

**UI elements:**
- Top toolbar row:
  - "Build" `QPushButton` (green) — starts `makepkg -si`
  - "Stop" `QPushButton` (red) — terminates process
  - Working directory label
- `QTextEdit` (read-only, monospace font) for live log output
- Bottom status bar: process state (Idle / Running / Finished / Failed) + exit code

**Build manager logic (`core/build_manager.py`):**
```python
class BuildManager(QObject):
    log_line = pyqtSignal(str)     # emitted for each output line
    finished = pyqtSignal(int)     # emitted with exit code

    def start(self, work_dir: str):
        # Use QProcess to run: makepkg -si
        # Connect readyReadStandardOutput / readyReadStandardError
        # Decode bytes to str, emit log_line signal

    def stop(self):
        # QProcess.terminate() -> kill() after 3s if still running
```

Log output color coding:
- Lines containing `error:` or `ERROR` — red
- Lines containing `warning:` or `WARNING` — yellow
- Lines starting with `==>` — bold/green (makepkg stage headers)
- All other lines — default color

---

## Main Window (`gui/main_window.py`)

```python
class MainWindow(QMainWindow):
    # Title: "TKG Kernel Builder"
    # Central widget: QTabWidget with 4 tabs
    # Status bar: shows linux-tkg submodule path + git status
    # Menu bar: File > Quit, Help > About
```

Window size: 900×700 minimum, resizable.

---

## `main.py`

```python
import sys
from PyQt6.QtWidgets import QApplication
from gui.main_window import MainWindow

def main():
    app = QApplication(sys.argv)
    app.setApplicationName("TKG GUI")
    app.setApplicationVersion("0.1.0")
    win = MainWindow()
    win.show()
    sys.exit(app.exec())

if __name__ == "__main__":
    main()
```

---

## `requirements.txt`

```
PyQt6>=6.4.0
requests>=2.28.0
```

---

## Implementation Order

1. Set up project structure (create all `__init__.py`, `requirements.txt`, `main.py`)
2. Fix `.gitmodules` (add wine-tkg-git, remove duplicate entry)
3. Implement `core/kernel_fetcher.py`
4. Implement `core/config_manager.py`
5. Implement `core/patch_manager.py`
6. Implement `core/build_manager.py`
7. Implement `gui/main_window.py`
8. Implement `gui/kernel_tab.py`
9. Implement `gui/config_tab.py`
10. Implement `gui/patches_tab.py`
11. Implement `gui/build_tab.py`
12. Commit and push to `claude/plan-kernel-gui-M25jK`
