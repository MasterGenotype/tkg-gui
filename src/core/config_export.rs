//! Export the configuration a build actually used into a directory.
//!
//! Two different things get called "the config", and an export is only useful if
//! it captures both:
//!
//! - **`customization.cfg`** — the tkg-gui-edited inputs (scheduler, compiler,
//!   optimisation level, feature presets).
//! - **the resolved kernel `.config`** — what those inputs, the base config file,
//!   any `*.myfrag` fragments and any menuconfig choices actually produced. This
//!   is the one that reproduces a build, and the one you feed back in as
//!   `_configfile` next time.
//!
//! The resolved `.config` lives in the unpacked kernel tree, which for tkg-gui is
//! inside a **temporary** work directory that is deleted when the app exits
//! ([`crate::core::work_dir::WorkDir`]). So without an export, the config that
//! built a working kernel is routinely thrown away. That is the point of this
//! module.
//!
//! Exports go into a timestamped subdirectory so a later one never overwrites an
//! earlier one, and a `MANIFEST.txt` records where each file came from.

use std::path::{Path, PathBuf};

/// Where the export went and what landed in it.
#[derive(Debug, Clone)]
pub struct ExportReport {
    /// The timestamped directory that was created.
    pub dir: PathBuf,
    /// `(filename, source path)` for each file copied.
    pub files: Vec<(String, PathBuf)>,
    /// Set when no resolved kernel `.config` could be found — the export still
    /// contains the inputs, but not the resolved output.
    pub missing_kernel_config: bool,
}

impl ExportReport {
    pub fn summary(&self) -> String {
        let note = if self.missing_kernel_config {
            "  (no resolved .config found — build or prepare the kernel first)"
        } else {
            ""
        };
        format!(
            "Exported {} file(s) to {}{}",
            self.files.len(),
            self.dir.display(),
            note
        )
    }
}

/// Whether `dir` looks like an unpacked kernel source tree.
///
/// A bare `.config` proves nothing — plenty of things are named that. A kernel
/// tree has a `Makefile` and a top-level `Kconfig` next to it.
fn is_kernel_tree(dir: &Path) -> bool {
    dir.join("Makefile").is_file() && dir.join("Kconfig").is_file()
}

/// Find the resolved kernel `.config`, newest first.
///
/// linux-tkg unpacks the kernel in different places depending on how it was
/// driven — `src/linux-<ver>` under makepkg, `linux-<ver>` or a
/// `_kernel_work_folder` under `install.sh` — so search the plausible roots
/// rather than assuming one. `extra_roots` carries anything read out of
/// `customization.cfg`.
pub fn find_kernel_config(linux_tkg_path: &Path, extra_roots: &[PathBuf]) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = vec![linux_tkg_path.join("src"), linux_tkg_path.to_path_buf()];
    roots.extend(extra_roots.iter().cloned());

    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in entries.filter_map(|e| e.ok()) {
            let dir = e.path();
            if !dir.is_dir() || !is_kernel_tree(&dir) {
                continue;
            }
            let cfg = dir.join(".config");
            if !cfg.is_file() {
                continue;
            }
            let when = cfg
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            candidates.push((when, cfg));
        }
    }
    pick_newest(candidates)
}

/// The most recently modified candidate.
///
/// Separated from the filesystem walk so the choice can be tested without
/// fighting mtime granularity: several kernel trees commonly coexist (an older
/// `linux-7.1` beside the `linux-7.2` just built), and picking the stale one
/// would export a config that never built anything.
fn pick_newest(candidates: Vec<(std::time::SystemTime, PathBuf)>) -> Option<PathBuf> {
    candidates
        .into_iter()
        .max_by_key(|(when, _)| *when)
        .map(|(_, p)| p)
}

/// Kernel-tree roots named by `customization.cfg`, so a custom
/// `_kernel_work_folder` is searched too.
pub fn extra_roots_from_cfg(cfg: &std::collections::HashMap<String, String>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for key in ["_kernel_work_folder", "_kernel_source_folder"] {
        if let Some(v) = cfg.get(key) {
            let v = v.trim().trim_matches('"');
            if v.is_empty() {
                continue;
            }
            let expanded = if let Some(rest) = v.strip_prefix("~/") {
                crate::settings::home_dir().join(rest)
            } else {
                PathBuf::from(v)
            };
            // The folder itself may be the tree, or hold `linux-<ver>` trees.
            out.push(expanded.clone());
            if let Some(parent) = expanded.parent() {
                out.push(parent.to_path_buf());
            }
        }
    }
    // linux-tkg's own default when install.sh drives the build.
    out.push(crate::settings::home_dir().join(".cache").join("linux-tkg"));
    out
}

/// Default place to put exports: the app data dir, which survives the temporary
/// work directory the kernel tree is unpacked into.
pub fn default_export_dir() -> PathBuf {
    crate::settings::AppSettings::data_dir().join("exports")
}

/// A filesystem-safe timestamp for the export directory name.
fn stamp() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

/// Copy the config inputs and the resolved `.config` into
/// `dest_dir/tkg-config-<timestamp>/`.
///
/// Never overwrites a previous export: the timestamped subdirectory is created
/// fresh, and an existing one is an error rather than a merge.
pub fn export(
    linux_tkg_path: &Path,
    dest_dir: &Path,
    cfg: &std::collections::HashMap<String, String>,
) -> Result<ExportReport, String> {
    let dir = dest_dir.join(format!("tkg-config-{}", stamp()));
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create {}: {}", dir.display(), e))?;

    let mut files: Vec<(String, PathBuf)> = Vec::new();

    let mut copy_in = |src: PathBuf, name: &str| -> Result<(), String> {
        std::fs::copy(&src, dir.join(name))
            .map_err(|e| format!("Could not copy {}: {}", src.display(), e))?;
        files.push((name.to_string(), src));
        Ok(())
    };

    // The inputs.
    let custom = linux_tkg_path.join("customization.cfg");
    if custom.is_file() {
        copy_in(custom, "customization.cfg")?;
    }

    // Fragments, which are part of how the resolved config came about.
    if let Ok(entries) = std::fs::read_dir(linux_tkg_path) {
        let mut frags: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "myfrag"))
            .collect();
        frags.sort();
        for f in frags {
            let name = f
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            copy_in(f, &name)?;
        }
    }

    // The resolved output.
    let kernel_cfg = find_kernel_config(linux_tkg_path, &extra_roots_from_cfg(cfg));
    let missing_kernel_config = kernel_cfg.is_none();
    if let Some(k) = kernel_cfg {
        copy_in(k, "kernel.config")?;
    }

    write_manifest(&dir, linux_tkg_path, cfg, &files, missing_kernel_config)?;

    Ok(ExportReport {
        dir,
        files,
        missing_kernel_config,
    })
}

/// Record provenance, so an export is still intelligible months later.
fn write_manifest(
    dir: &Path,
    linux_tkg_path: &Path,
    cfg: &std::collections::HashMap<String, String>,
    files: &[(String, PathBuf)],
    missing_kernel_config: bool,
) -> Result<(), String> {
    let mut m = String::new();
    m.push_str("# tkg-gui config export\n");
    m.push_str(&format!(
        "exported-at: {}\n",
        chrono::Utc::now().to_rfc3339()
    ));
    m.push_str(&format!("linux-tkg-path: {}\n", linux_tkg_path.display()));
    for key in [
        "_version",
        "_cpusched",
        "_compiler",
        "_compileroptlevel",
        "_processor_opt",
        "_configfile",
    ] {
        if let Some(v) = cfg.get(key) {
            m.push_str(&format!("{key}: {}\n", v.trim().trim_matches('"')));
        }
    }
    if missing_kernel_config {
        m.push_str(
            "\n# NOTE: no resolved kernel .config was found. This export holds the\n\
             # inputs only. Run a build (or at least the prepare step) and export\n\
             # again to capture the resolved config.\n",
        );
    }
    m.push_str("\nfiles:\n");
    for (name, src) in files {
        m.push_str(&format!("  {name}  <-  {}\n", src.display()));
    }
    std::fs::write(dir.join("MANIFEST.txt"), m)
        .map_err(|e| format!("Could not write MANIFEST.txt: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tree(tag: &str) -> Tmp {
        let d = std::env::temp_dir().join(format!("tkg-gui-export-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("linux-tkg")).unwrap();
        std::fs::create_dir_all(d.join("out")).unwrap();
        std::fs::write(
            d.join("linux-tkg/customization.cfg"),
            "_cpusched=\"bore\"\n_version=\"7.2\"\n",
        )
        .unwrap();
        Tmp(d)
    }

    /// Plant a kernel-shaped tree with a `.config` inside it.
    fn kernel_tree(at: &Path, body: &str) {
        std::fs::create_dir_all(at).unwrap();
        std::fs::write(at.join("Makefile"), "VERSION = 7\n").unwrap();
        std::fs::write(at.join("Kconfig"), "mainmenu\n").unwrap();
        std::fs::write(at.join(".config"), body).unwrap();
    }

    #[test]
    fn a_bare_dot_config_is_not_mistaken_for_a_kernel_tree() {
        let t = tree("notkernel");
        let fake = t.0.join("linux-tkg/src/not-a-kernel");
        std::fs::create_dir_all(&fake).unwrap();
        std::fs::write(fake.join(".config"), "nope").unwrap();

        assert_eq!(
            find_kernel_config(&t.0.join("linux-tkg"), &[]),
            None,
            "a .config without Makefile+Kconfig must not be picked up"
        );
    }

    #[test]
    fn finds_the_config_in_a_makepkg_src_tree() {
        let t = tree("makepkg");
        kernel_tree(&t.0.join("linux-tkg/src/linux-7.2"), "CONFIG_A=y\n");
        let found = find_kernel_config(&t.0.join("linux-tkg"), &[]).expect("should find it");
        assert!(found.ends_with("linux-7.2/.config"), "{found:?}");
    }

    /// An older kernel tree routinely sits beside the one just built; exporting
    /// the stale config would capture something that never built anything.
    #[test]
    fn the_newest_candidate_wins() {
        use std::time::{Duration, SystemTime};
        let base = SystemTime::UNIX_EPOCH;
        let picked = pick_newest(vec![
            (
                base + Duration::from_secs(100),
                PathBuf::from("/old/.config"),
            ),
            (
                base + Duration::from_secs(900),
                PathBuf::from("/new/.config"),
            ),
            (
                base + Duration::from_secs(500),
                PathBuf::from("/mid/.config"),
            ),
        ]);
        assert_eq!(picked, Some(PathBuf::from("/new/.config")));
        assert_eq!(pick_newest(vec![]), None);
    }

    #[test]
    fn several_kernel_trees_are_all_considered() {
        let t = tree("several");
        kernel_tree(&t.0.join("linux-tkg/src/linux-7.1"), "OLD\n");
        kernel_tree(&t.0.join("linux-tkg/src/linux-7.2"), "NEW\n");
        // Both are valid candidates; the selection itself is covered above.
        let found = find_kernel_config(&t.0.join("linux-tkg"), &[]).expect("should find one");
        let body = std::fs::read_to_string(&found).unwrap();
        assert!(body == "OLD\n" || body == "NEW\n", "{found:?}");
    }

    #[test]
    fn export_gathers_inputs_fragments_and_the_resolved_config() {
        let t = tree("full");
        let tkg = t.0.join("linux-tkg");
        std::fs::write(tkg.join("tkg-gui-xen.myfrag"), "CONFIG_XEN=y\n").unwrap();
        std::fs::write(tkg.join("tkg-gui-lvm-thin.myfrag"), "CONFIG_DM_THIN=y\n").unwrap();
        kernel_tree(&tkg.join("src/linux-7.2"), "CONFIG_RESOLVED=y\n");

        let r = export(&tkg, &t.0.join("out"), &HashMap::new()).unwrap();
        assert!(!r.missing_kernel_config);

        let names: Vec<&str> = r.files.iter().map(|(n, _)| n.as_str()).collect();
        for want in [
            "customization.cfg",
            "tkg-gui-lvm-thin.myfrag",
            "tkg-gui-xen.myfrag",
            "kernel.config",
        ] {
            assert!(names.contains(&want), "missing {want} in {names:?}");
        }
        assert_eq!(
            std::fs::read_to_string(r.dir.join("kernel.config")).unwrap(),
            "CONFIG_RESOLVED=y\n"
        );
        assert!(r.dir.join("MANIFEST.txt").is_file());
    }

    #[test]
    fn export_still_succeeds_without_a_resolved_config_and_says_so() {
        let t = tree("noconfig");
        let r = export(&t.0.join("linux-tkg"), &t.0.join("out"), &HashMap::new()).unwrap();
        assert!(r.missing_kernel_config);
        assert!(r.summary().contains("no resolved .config"));
        // The inputs are still captured — that is the point of not failing.
        assert!(r.dir.join("customization.cfg").is_file());
        let manifest = std::fs::read_to_string(r.dir.join("MANIFEST.txt")).unwrap();
        assert!(manifest.contains("no resolved kernel .config was found"));
    }

    /// Two exports must not collide; losing an earlier one defeats the purpose.
    #[test]
    fn manifest_records_provenance_and_build_inputs() {
        let t = tree("manifest");
        let cfg: HashMap<String, String> = [
            ("_cpusched".to_string(), "\"bore\"".to_string()),
            ("_compileroptlevel".to_string(), "\"2\"".to_string()),
        ]
        .into_iter()
        .collect();
        let r = export(&t.0.join("linux-tkg"), &t.0.join("out"), &cfg).unwrap();
        let m = std::fs::read_to_string(r.dir.join("MANIFEST.txt")).unwrap();
        assert!(m.contains("_cpusched: bore"), "{m}");
        assert!(m.contains("_compileroptlevel: 2"), "{m}");
        assert!(m.contains("linux-tkg-path:"));
        assert!(m.contains("customization.cfg  <-"));
    }

    #[test]
    fn extra_roots_expand_a_tilde_and_include_the_parent() {
        let cfg: HashMap<String, String> = [(
            "_kernel_work_folder".to_string(),
            "~/kernels/linux-7.2".to_string(),
        )]
        .into_iter()
        .collect();
        let roots = extra_roots_from_cfg(&cfg);
        let as_str: Vec<String> = roots.iter().map(|p| p.display().to_string()).collect();
        assert!(
            as_str.iter().any(|s| s.ends_with("kernels/linux-7.2")),
            "{as_str:?}"
        );
        assert!(as_str.iter().any(|s| s.ends_with("kernels")), "{as_str:?}");
        assert!(
            !as_str.iter().any(|s| s.starts_with('~')),
            "tilde must expand"
        );
    }
}

#[cfg(test)]
mod real_tree {
    use super::*;

    /// Export a real-shaped tree and print the result, for eyeballing what the
    /// button produces:
    ///
    /// ```text
    /// TKG_GUI_EXPORT_FROM=/path/to/linux-tkg TKG_GUI_EXPORT_TO=/tmp/out \
    ///   cargo test export_a_real_tree -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs TKG_GUI_EXPORT_FROM and TKG_GUI_EXPORT_TO"]
    fn export_a_real_tree() {
        let (Ok(from), Ok(to)) = (
            std::env::var("TKG_GUI_EXPORT_FROM"),
            std::env::var("TKG_GUI_EXPORT_TO"),
        ) else {
            return;
        };
        let from = PathBuf::from(from);
        let cfg: std::collections::HashMap<String, String> =
            std::fs::read_to_string(from.join("customization.cfg"))
                .unwrap_or_default()
                .lines()
                .filter_map(|l| l.split_once('='))
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .collect();

        let r = export(&from, Path::new(&to), &cfg).expect("export failed");
        println!("{}", r.summary());
        for (name, src) in &r.files {
            println!("  {name}  <-  {}", src.display());
        }
        println!(
            "\n--- MANIFEST.txt ---\n{}",
            std::fs::read_to_string(r.dir.join("MANIFEST.txt")).unwrap()
        );
        assert!(!r.missing_kernel_config, "should have found the .config");
    }
}
