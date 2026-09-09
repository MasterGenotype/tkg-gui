//! Automated kernel config fragments (`.myfrag`) for linux-tkg.
//!
//! linux-tkg merges any `*.myfrag` next to the PKGBUILD when
//! `_config_fragments=true` via `scripts/kconfig/merge_config.sh`.

use std::fs;
use std::path::Path;

const XEN_FRAG: &str = "tkg-gui-xen.myfrag";
const LVM_THIN_FRAG: &str = "tkg-gui-lvm-thin.myfrag";
const ACPI_CALL_FRAG: &str = "tkg-gui-acpi-call.myfrag";
const ACPI_CALL_HELPER: &str = "tkg-gui-acpi-call-install.sh";

/// GUI-managed feature presets that materialize as linux-tkg config fragments.
#[derive(Debug, Clone, Default)]
pub struct FeaturePresets {
    pub xen_dom0: bool,
    pub lvm_thin: bool,
    /// Ensures ACPI options needed for out-of-tree `acpi_call` and writes a DKMS helper script.
    pub acpi_call: bool,
}

impl FeaturePresets {
    pub fn from_map(values: &std::collections::HashMap<String, String>) -> Self {
        Self {
            xen_dom0: is_truthy(values.get("_tkg_gui_xen_dom0")),
            lvm_thin: is_truthy(values.get("_tkg_gui_lvm_thin")),
            acpi_call: is_truthy(values.get("_tkg_gui_acpi_call")),
        }
    }

    pub fn apply_to_map(&self, values: &mut std::collections::HashMap<String, String>) {
        values.insert(
            "_tkg_gui_xen_dom0".into(),
            bool_str(self.xen_dom0).into(),
        );
        values.insert("_tkg_gui_lvm_thin".into(), bool_str(self.lvm_thin).into());
        values.insert(
            "_tkg_gui_acpi_call".into(),
            bool_str(self.acpi_call).into(),
        );
        // Auto-enable silent fragment apply so builds don't hang on prompts
        if self.xen_dom0 || self.lvm_thin || self.acpi_call {
            values.insert("_config_fragments".into(), "true".into());
            values.insert("_config_fragments_no_confirm".into(), "true".into());
        }
    }
}

fn is_truthy(v: Option<&String>) -> bool {
    matches!(
        v.map(|s| s.as_str()),
        Some("true" | "1" | "yes" | "y" | "on")
    )
}

fn bool_str(v: bool) -> &'static str {
    if v {
        "true"
    } else {
        "false"
    }
}

/// Sync `.myfrag` files (and helpers) into the linux-tkg work directory.
pub fn sync_fragments(linux_tkg_path: &Path, presets: &FeaturePresets) -> Result<Vec<String>, String> {
    let mut actions = Vec::new();

    set_fragment(
        linux_tkg_path,
        XEN_FRAG,
        presets.xen_dom0,
        XEN_MYFRAG,
        &mut actions,
    )?;
    set_fragment(
        linux_tkg_path,
        LVM_THIN_FRAG,
        presets.lvm_thin,
        LVM_THIN_MYFRAG,
        &mut actions,
    )?;
    set_fragment(
        linux_tkg_path,
        ACPI_CALL_FRAG,
        presets.acpi_call,
        ACPI_CALL_MYFRAG,
        &mut actions,
    )?;

    let helper = linux_tkg_path.join(ACPI_CALL_HELPER);
    if presets.acpi_call {
        fs::write(&helper, ACPI_CALL_INSTALL_SH)
            .map_err(|e| format!("write {}: {}", helper.display(), e))?;
        // best-effort executable bit
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&helper, fs::Permissions::from_mode(0o755));
        }
        actions.push(format!("wrote {}", ACPI_CALL_HELPER));
    } else if helper.exists() {
        let _ = fs::remove_file(&helper);
        actions.push(format!("removed {}", ACPI_CALL_HELPER));
    }

    Ok(actions)
}

fn set_fragment(
    dir: &Path,
    name: &str,
    enable: bool,
    contents: &str,
    actions: &mut Vec<String>,
) -> Result<(), String> {
    let path = dir.join(name);
    if enable {
        fs::write(&path, contents).map_err(|e| format!("write {}: {}", path.display(), e))?;
        actions.push(format!("wrote {}", name));
    } else if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("remove {}: {}", path.display(), e))?;
        actions.push(format!("removed {}", name));
    }
    Ok(())
}

// --- Fragment contents -------------------------------------------------------

/// Dom0-oriented Xen guest/host kernel options.
/// Base linux-tkg configs often already enable many of these; the fragment
/// forces them on when using custom/minimal `_configfile` sources.
const XEN_MYFRAG: &str = r#"# tkg-gui: Xen dom0 / backend support
# Applied via linux-tkg config fragments (*.myfrag)
CONFIG_XEN=y
CONFIG_XEN_DOM0=y
CONFIG_XEN_PV=y
CONFIG_XEN_PV_SMP=y
CONFIG_XEN_PV_DOM0=y
CONFIG_XEN_PVHVM=y
CONFIG_XEN_PVHVM_SMP=y
CONFIG_XEN_PVHVM_GUEST=y
CONFIG_XEN_PVH=y
CONFIG_XEN_SAVE_RESTORE=y
CONFIG_XEN_512GB=y
CONFIG_XEN_PV_MSR_SAFE=y
CONFIG_PCI_XEN=y
CONFIG_XEN_PCIDEV_FRONTEND=m
CONFIG_XEN_PCIDEV_BACKEND=m
CONFIG_XEN_PCI_STUB=y
CONFIG_XEN_BLKDEV_FRONTEND=m
CONFIG_XEN_BLKDEV_BACKEND=m
CONFIG_XEN_NETDEV_FRONTEND=m
CONFIG_XEN_NETDEV_BACKEND=m
CONFIG_XEN_SCSI_FRONTEND=m
CONFIG_XEN_SCSI_BACKEND=m
CONFIG_INPUT_XEN_KBDDEV_FRONTEND=m
CONFIG_HVC_DRIVER=y
CONFIG_HVC_IRQ=y
CONFIG_HVC_XEN=y
CONFIG_HVC_XEN_FRONTEND=y
CONFIG_XEN_FBDEV_FRONTEND=m
CONFIG_XEN_WDT=m
CONFIG_XEN_BALLOON=y
CONFIG_XEN_BALLOON_MEMORY_HOTPLUG=y
CONFIG_XEN_SCRUB_PAGES_DEFAULT=y
CONFIG_XEN_DEV_EVTCHN=m
CONFIG_XEN_BACKEND=y
CONFIG_XENFS=m
CONFIG_XEN_COMPAT_XENFS=y
CONFIG_XEN_SYS_HYPERVISOR=y
CONFIG_XEN_XENBUS_FRONTEND=y
CONFIG_XEN_GNTDEV=m
CONFIG_XEN_GNTDEV_DMABUF=y
CONFIG_XEN_GRANT_DEV_ALLOC=m
CONFIG_XEN_GRANT_DMA_ALLOC=y
CONFIG_SWIOTLB_XEN=y
CONFIG_XEN_PRIVCMD=m
CONFIG_XEN_PRIVCMD_EVENTFD=y
CONFIG_XEN_ACPI_PROCESSOR=m
CONFIG_XEN_MCE_LOG=y
CONFIG_XEN_EFI=y
CONFIG_XEN_AUTO_XLATE=y
CONFIG_XEN_ACPI=y
CONFIG_XEN_SYMS=y
CONFIG_XEN_UNPOPULATED_ALLOC=y
CONFIG_XEN_GRANT_DMA_OPS=y
CONFIG_XEN_VIRTIO=y
CONFIG_XEN_PVCALLS_FRONTEND=m
CONFIG_XEN_PVCALLS_BACKEND=m
CONFIG_MEMORY_HOTPLUG=y
CONFIG_MEMORY_HOTREMOVE=y
"#;

/// Device-mapper thin provisioning stack for LVM thin pools.
const LVM_THIN_MYFRAG: &str = r#"# tkg-gui: LVM thin provisioning
CONFIG_MD=y
CONFIG_BLK_DEV_DM=y
CONFIG_BLK_DEV_DM_BUILTIN=y
CONFIG_DM_BUFIO=y
CONFIG_DM_BIO_PRISON=y
CONFIG_DM_PERSISTENT_DATA=y
CONFIG_DM_THIN_PROVISIONING=y
CONFIG_DM_CACHE=m
CONFIG_DM_CACHE_SMQ=m
CONFIG_DM_MIRROR=m
CONFIG_DM_ZERO=m
CONFIG_DM_SNAPSHOT=m
CONFIG_DM_RAID=m
CONFIG_DM_THIN_PROVISIONING=y
"#;

/// Kernel options commonly required to build/run out-of-tree acpi_call.
const ACPI_CALL_MYFRAG: &str = r#"# tkg-gui: prerequisites for out-of-tree acpi_call module
CONFIG_ACPI=y
CONFIG_ACPI_DEBUG=y
CONFIG_X86_MSR=y
CONFIG_MODULES=y
CONFIG_MODULE_UNLOAD=y
"#;

const ACPI_CALL_INSTALL_SH: &str = r#"#!/usr/bin/env bash
# tkg-gui helper: build/install out-of-tree acpi_call against this kernel
# Usage (as root, after the tkg kernel is installed and booted, or with headers):
#   ./tkg-gui-acpi-call-install.sh
set -euo pipefail

KVER="${1:-$(uname -r)}"
SRC_DIR="${ACPI_CALL_SRC:-/usr/src/acpi_call}"
WORKDIR="${TMPDIR:-/tmp}/tkg-gui-acpi-call-$$"

echo "==> acpi_call for kernel ${KVER}"

if command -v pacman >/dev/null 2>&1; then
  if pacman -Si acpi_call-dkms >/dev/null 2>&1; then
    echo "==> Installing acpi_call-dkms via pacman"
    pacman -S --needed --noconfirm acpi_call-dkms
    echo "==> Done (DKMS). Load with: modprobe acpi_call"
    exit 0
  fi
fi

if command -v apt-get >/dev/null 2>&1; then
  if apt-cache show acpi-call-dkms >/dev/null 2>&1; then
    echo "==> Installing acpi-call-dkms via apt"
    apt-get install -y acpi-call-dkms
    echo "==> Done (DKMS). Load with: modprobe acpi_call"
    exit 0
  fi
fi

echo "==> No distro DKMS package found; building from source"
rm -rf "$WORKDIR"
mkdir -p "$WORKDIR"
git clone --depth 1 https://github.com/nix-community/acpi_call.git "$WORKDIR/acpi_call"
cd "$WORKDIR/acpi_call"
make
make install
depmod -a "$KVER" || true
echo "==> Built and installed. Load with: modprobe acpi_call"
echo "    Test: echo '\_SB.PCI0.PEG0.PEGP._OFF' > /proc/acpi/call"
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn writes_and_removes_frags() {
        let dir = std::env::temp_dir().join(format!("tkg-gui-frag-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut map = HashMap::new();
        map.insert("_tkg_gui_xen_dom0".into(), "true".into());
        map.insert("_tkg_gui_lvm_thin".into(), "true".into());
        map.insert("_tkg_gui_acpi_call".into(), "true".into());
        let p = FeaturePresets::from_map(&map);
        let actions = sync_fragments(&dir, &p).unwrap();
        assert!(actions.iter().any(|a| a.contains("xen")));
        assert!(dir.join("tkg-gui-xen.myfrag").exists());
        assert!(dir.join("tkg-gui-lvm-thin.myfrag").exists());
        assert!(dir.join("tkg-gui-acpi-call.myfrag").exists());
        assert!(dir.join("tkg-gui-acpi-call-install.sh").exists());
        let contents = std::fs::read_to_string(dir.join("tkg-gui-xen.myfrag")).unwrap();
        assert!(contents.contains("CONFIG_XEN_DOM0=y"));
        let thin = std::fs::read_to_string(dir.join("tkg-gui-lvm-thin.myfrag")).unwrap();
        assert!(thin.contains("CONFIG_DM_THIN_PROVISIONING=y"));
        // disable
        let off = FeaturePresets::default();
        sync_fragments(&dir, &off).unwrap();
        assert!(!dir.join("tkg-gui-xen.myfrag").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
