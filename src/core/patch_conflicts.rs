//! Detect user patches that duplicate a bundled linux-tkg patch.
//!
//! A `.mypatch` that re-adds something linux-tkg already patches in does not
//! fail to apply — both patches land, and the build dies much later in the
//! compile. The canonical example:
//!
//! ```text
//! kernel/fork.c:138:12: error: static declaration of 'unprivileged_userns_clone'
//!                              follows non-static declaration
//! ```
//!
//! caused by Arch's `add-sysctl-to-allow-disabling-unprivileged-CLONE_NEWUSER`
//! userpatch (which defines the symbol `static` in `kernel/fork.c`) landing on
//! top of linux-tkg's bundled `0001-add-sysctl-to-disallow-...` (which declares
//! it `extern` in a header and defines it in `kernel/user_namespace.c`). Both
//! provide the same sysctl; keeping both is never what anyone wants.
//!
//! Userpatches are applied *after* every bundled patch (`prepare:1825`), so the
//! duplicate is always the userpatch — which is also the one the user can
//! simply delete.
//!
//! This scans for three classes of collision, each one a thing that can only be
//! declared once:
//!
//! | Class | Looks for | Symptom when duplicated |
//! |-------|-----------|-------------------------|
//! | Kconfig symbol | `+config NAME` | duplicate symbol, doubled menu entry |
//! | sysctl procname | `+ .procname = "name"` | duplicate sysctl registration |
//! | file-scope definition | `+int name =` at column 0 | redefinition / static-vs-extern |
//!
//! It deliberately does **not** try to predict whether two patches' hunks will
//! textually conflict — `patch` reports that itself, immediately and clearly.
//! The point here is the failure mode `patch` stays silent about.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What kind of thing is declared twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolKind {
    Kconfig,
    Sysctl,
    Definition,
}

impl SymbolKind {
    pub fn label(self) -> &'static str {
        match self {
            SymbolKind::Kconfig => "Kconfig symbol",
            SymbolKind::Sysctl => "sysctl procname",
            SymbolKind::Definition => "file-scope definition",
        }
    }
}

/// Whether the bundled patch is actually applied for the current config.
///
/// Most bundled patches are gated on a `customization.cfg` key, so a collision
/// against one that is switched off is latent rather than live. Saying which is
/// the difference between a warning worth acting on and noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applicability {
    /// The config selects this bundled patch — the collision is live.
    Applied,
    /// The config does not select it — latent, would bite if settings change.
    NotApplied,
    /// No gate is known for this patch, or the deciding key is unset.
    Unknown,
}

/// One duplicated declaration.
#[derive(Debug, Clone)]
pub struct Collision {
    pub kind: SymbolKind,
    pub symbol: String,
    pub user_patch: String,
    pub bundled_patch: String,
    pub bundled_applies: Applicability,
}

/// Added lines of a unified diff, with the leading `+` stripped.
///
/// `+++` headers are skipped: they are metadata, and `+++ b/kernel/fork.c`
/// would otherwise parse as added content.
fn added_lines(patch_text: &str) -> impl Iterator<Item = &str> {
    patch_text
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .map(|l| &l[1..])
}

/// `config FOO` Kconfig symbol definitions among added lines.
fn kconfig_symbols(patch_text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in added_lines(patch_text) {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("config ") {
            let name = rest.trim();
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                out.insert(name.to_string());
            }
        }
    }
    out
}

/// `.procname = "foo"` sysctl names among added lines.
fn sysctl_procnames(patch_text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in added_lines(patch_text) {
        if let Some(idx) = line.find(".procname") {
            let rest = &line[idx..];
            if let Some(open) = rest.find('"') {
                if let Some(close) = rest[open + 1..].find('"') {
                    let name = &rest[open + 1..open + 1 + close];
                    if !name.is_empty() {
                        out.insert(name.to_string());
                    }
                }
            }
        }
    }
    out
}

/// C file-scope variable definitions among added lines.
///
/// Column 0 only — an indented declaration is a local, which may legitimately
/// share a name with anything. This is what keeps the check from drowning in
/// false positives on a 199-patch series.
fn file_scope_definitions(patch_text: &str) -> BTreeSet<String> {
    const TYPES: &[&str] = &[
        "int", "long", "bool", "char", "short", "u8", "u16", "u32", "u64", "s32", "s64", "size_t",
        "unsigned", "void",
    ];
    let mut out = BTreeSet::new();
    for line in added_lines(patch_text) {
        // Column 0: no leading whitespace, and not a preprocessor directive.
        if line.starts_with(char::is_whitespace) || line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut toks = line.split_whitespace().peekable();
        // Skip leading qualifiers.
        let mut saw_type = false;
        let mut name: Option<&str> = None;
        for tok in toks.by_ref() {
            match tok {
                "static" | "const" | "volatile" | "extern" | "signed" | "unsigned" => continue,
                t if TYPES.contains(&t) => {
                    saw_type = true;
                    continue;
                }
                other if saw_type => {
                    // Strip pointer stars and any trailing `=`/`;`/`[`.
                    let ident: String = other
                        .trim_start_matches('*')
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if !ident.is_empty() {
                        name = Some(other);
                        // Only count it when the line really is a definition:
                        // an `=` or a `;` terminates it. A `(` means function.
                        let tail = &line[line.find(other).unwrap_or(0) + other.len()..];
                        let is_def = other.ends_with(';')
                            || other.ends_with('=')
                            || tail.trim_start().starts_with('=')
                            || tail.trim_start().starts_with(';');
                        let is_fn = other.contains('(') || tail.trim_start().starts_with('(');
                        if is_def && !is_fn {
                            out.insert(ident);
                        }
                    }
                    break;
                }
                _ => break,
            }
        }
        let _ = name;
    }
    out
}

/// Whether a bundled patch is selected by this `customization.cfg`.
///
/// Mirrors the conditions in `linux-tkg-config/prepare`. Anything not listed
/// here is [`Applicability::Unknown`] — reported, but flagged as uncertain
/// rather than asserted.
fn bundled_applies(patch_file: &str, cfg: &BTreeMap<String, String>) -> Applicability {
    let get = |k: &str| cfg.get(k).map(|s| s.trim().to_ascii_lowercase());
    let is_true = |k: &str| match get(k).as_deref() {
        Some("true") | Some("1") | Some("yes") => Applicability::Applied,
        Some("false") | Some("0") | Some("no") => Applicability::NotApplied,
        _ => Applicability::Unknown,
    };
    let eq = |k: &str, v: &str| match get(k) {
        Some(got) if got == v => Applicability::Applied,
        Some(got) if got.is_empty() => Applicability::Unknown,
        Some(_) => Applicability::NotApplied,
        None => Applicability::Unknown,
    };

    // `prepare:788` — hardened replaces the Arch userns patch, and only when
    // both the hardened config file and the cfs scheduler are selected.
    let hardened = matches!(
        (get("_configfile").as_deref(), get("_cpusched").as_deref()),
        (Some("config_hardened.x86_64"), Some("cfs"))
    );

    match patch_file {
        "0001-add-sysctl-to-disallow-unprivileged-CLONE_NEWUSER-by.patch" => {
            if hardened {
                Applicability::NotApplied
            } else {
                Applicability::Applied
            }
        }
        "0012-linux-hardened.patch" => {
            if hardened {
                Applicability::Applied
            } else {
                Applicability::NotApplied
            }
        }
        "0013-optimize_harder_O3.patch" => eq("_compileroptlevel", "2"),
        "0001-bore.patch" => eq("_cpusched", "bore"),
        "0004-muqss.patch" => eq("_cpusched", "muqss"),
        "0009-prjc.patch" => match get("_cpusched").as_deref() {
            Some("pds") | Some("bmq") => Applicability::Applied,
            Some("") | None => Applicability::Unknown,
            Some(_) => Applicability::NotApplied,
        },
        "0002-clear-patches.patch" => is_true("_clear_patches"),
        "0003-glitched-base.patch" => is_true("_glitched_base"),
        "0012-misc-additions.patch" => is_true("_misc_adds"),
        "0006-add-acs-overrides_iommu.patch" => is_true("_acs_override"),
        "0014-OpenRGB.patch" => is_true("_openrgb"),
        "0013-suse-additions.patch" => eq("_distro", "suse"),
        f if f.starts_with("0013-gentoo-") => eq("_distro", "gentoo"),
        _ => Applicability::Unknown,
    }
}

/// Every collision between one userpatch and one bundled patch, collapsed into
/// a single finding.
///
/// One duplicated patch usually trips several classes at once — the userns case
/// collides on both the sysctl name and the definition — and reporting those as
/// separate findings makes one problem look like several. The actionable unit is
/// the *pair*, because the fix is to delete one file.
#[derive(Debug, Clone)]
pub struct PairReport {
    pub user_patch: String,
    pub bundled_patch: String,
    pub applies: Applicability,
    /// Duplicated declarations, deduplicated and ordered by class.
    pub symbols: Vec<(SymbolKind, String)>,
}

impl PairReport {
    /// Whether this finding is live for the current configuration.
    pub fn is_live(&self) -> bool {
        self.applies != Applicability::NotApplied
    }

    /// Multi-part rendering: headline, then the duplicated declarations.
    pub fn headline(&self) -> String {
        let note = match self.applies {
            Applicability::Applied => "",
            Applicability::NotApplied => "  (bundled patch NOT active for this config)",
            Applicability::Unknown => "  (bundled patch may not be active)",
        };
        format!(
            "{} duplicates bundled {}{}",
            self.user_patch, self.bundled_patch, note
        )
    }

    /// `unprivileged_userns_clone (sysctl procname, file-scope definition)`
    pub fn detail(&self) -> String {
        let mut by_sym: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (k, s) in &self.symbols {
            by_sym.entry(s.as_str()).or_default().push(k.label());
        }
        by_sym
            .into_iter()
            .map(|(s, kinds)| format!("{} ({})", s, kinds.join(", ")))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Collapse raw collisions into one finding per patch pair, live findings first.
pub fn group(collisions: &[Collision]) -> Vec<PairReport> {
    let mut map: BTreeMap<(String, String), PairReport> = BTreeMap::new();
    for c in collisions {
        let key = (c.user_patch.clone(), c.bundled_patch.clone());
        let e = map.entry(key).or_insert_with(|| PairReport {
            user_patch: c.user_patch.clone(),
            bundled_patch: c.bundled_patch.clone(),
            applies: c.bundled_applies,
            symbols: Vec::new(),
        });
        let pair = (c.kind, c.symbol.clone());
        if !e.symbols.contains(&pair) {
            e.symbols.push(pair);
        }
    }
    let mut out: Vec<PairReport> = map.into_values().collect();
    for r in &mut out {
        r.symbols.sort();
    }
    // Live findings first; they are the ones that will break this build.
    out.sort_by_key(|r| (!r.is_live(), r.user_patch.clone(), r.bundled_patch.clone()));
    out
}

/// `customization.cfg` key: disable conflicting userpatches at build start.
///
/// GUI-only, ignored by linux-tkg itself, following the `_tkg_gui_*` convention
/// already used by the feature presets.
pub const AUTOFIX_KEY: &str = "_tkg_gui_autofix_conflicts";

/// What happened to one offending userpatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Renamed to `*.disabled`; linux-tkg's `*.mypatch` glob no longer matches.
    Disabled { name: String, to: String },
    /// Left alone, with the reason.
    Skipped { name: String, why: String },
    /// The rename failed.
    Failed { name: String, why: String },
}

impl Resolution {
    pub fn summary(&self) -> String {
        match self {
            Resolution::Disabled { name, to } => format!("disabled {name} -> {to}"),
            Resolution::Skipped { name, why } => format!("skipped {name}: {why}"),
            Resolution::Failed { name, why } => format!("FAILED to disable {name}: {why}"),
        }
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, Resolution::Failed { .. })
    }

    pub fn disabled_name(&self) -> Option<&str> {
        match self {
            Resolution::Disabled { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }
}

/// The userpatch filenames implicated in `findings`, deduplicated.
///
/// Only the *user* side is ever returned. The bundled patch belongs to the
/// linux-tkg checkout and is chosen by `customization.cfg`, so it is never the
/// thing to remove. With `live_only`, findings against a bundled patch this
/// config does not select are excluded — those are latent, and disabling
/// someone's patch over a conflict that is not actually happening would be
/// wrong.
pub fn offending_user_patches(findings: &[PairReport], live_only: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in findings {
        if live_only && !f.is_live() {
            continue;
        }
        if !out.contains(&f.user_patch) {
            out.push(f.user_patch.clone());
        }
    }
    out
}

/// Disable the named userpatches by renaming each to `<name>.disabled`.
///
/// Renaming rather than deleting, deliberately: it is the same mechanism the
/// Patches tab's per-patch toggle already uses, it is trivially undoable, and a
/// patch set someone curated is not ours to destroy. [`scan`] skips
/// `*.disabled`, so a re-scan afterwards comes back clean.
///
/// Each name is resolved strictly inside the userpatch directory; anything
/// carrying a path separator or `..` is refused rather than followed.
pub fn disable_user_patches(
    linux_tkg_path: &Path,
    kernel_series: &str,
    names: &[String],
) -> Vec<Resolution> {
    let dir = crate::core::patch_manager::get_patch_dir(linux_tkg_path, kernel_series);
    let mut out = Vec::new();
    for name in names {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
            out.push(Resolution::Skipped {
                name: name.clone(),
                why: "not a plain filename".into(),
            });
            continue;
        }
        let from = dir.join(name);
        if !from.is_file() {
            out.push(Resolution::Skipped {
                name: name.clone(),
                why: "no longer present".into(),
            });
            continue;
        }
        let to_name = format!("{name}.disabled");
        let to = dir.join(&to_name);
        if to.exists() {
            out.push(Resolution::Skipped {
                name: name.clone(),
                why: format!("{to_name} already exists"),
            });
            continue;
        }
        match std::fs::rename(&from, &to) {
            Ok(()) => out.push(Resolution::Disabled {
                name: name.clone(),
                to: to_name,
            }),
            Err(e) => out.push(Resolution::Failed {
                name: name.clone(),
                why: e.to_string(),
            }),
        }
    }
    out
}

/// Whether the auto-disable option is switched on in `customization.cfg`.
pub fn autofix_enabled(cfg: &BTreeMap<String, String>) -> bool {
    matches!(
        cfg.get(AUTOFIX_KEY)
            .map(|s| s.trim().to_ascii_lowercase())
            .as_deref(),
        Some("true") | Some("1") | Some("yes")
    )
}

/// Directory holding the bundled patches for `kernel_series` (dotted, "7.2").
pub fn bundled_patch_dir(linux_tkg_path: &Path, kernel_series: &str) -> PathBuf {
    linux_tkg_path.join("linux-tkg-patches").join(kernel_series)
}

/// The kernel series to check, as the bundled-patch directories name it.
///
/// Prefers `_version` from `customization.cfg`; falls back to the single
/// `linux<ver>-tkg-userpatches` directory present, since that is the one whose
/// patches would actually be applied. `None` when neither resolves to a real
/// bundled-patch directory — better to check nothing than to check the wrong
/// kernel's patches.
pub fn resolve_series(linux_tkg_path: &Path, cfg: &BTreeMap<String, String>) -> Option<String> {
    if let Some(v) = cfg.get("_version").map(|s| s.trim()) {
        if !v.is_empty() && bundled_patch_dir(linux_tkg_path, v).is_dir() {
            return Some(v.to_string());
        }
    }
    // Derive from the userpatches directory: linux72-tkg-userpatches -> 7.2.
    let entries = std::fs::read_dir(linux_tkg_path).ok()?;
    for e in entries.filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(rest) = name
            .strip_prefix("linux")
            .and_then(|r| r.strip_suffix("-tkg-userpatches"))
        {
            if rest.len() >= 2 && rest.chars().all(|c| c.is_ascii_digit()) {
                let dotted = format!("{}.{}", &rest[..1], &rest[1..]);
                if bundled_patch_dir(linux_tkg_path, &dotted).is_dir() {
                    return Some(dotted);
                }
                let dotted2 = format!("{}.{}", &rest[..2], &rest[2..]);
                if rest.len() >= 3 && bundled_patch_dir(linux_tkg_path, &dotted2).is_dir() {
                    return Some(dotted2);
                }
            }
        }
    }
    None
}

/// Every declaration a patch adds, across all three classes.
fn symbols_of(text: &str) -> Vec<(SymbolKind, String)> {
    let mut out = Vec::new();
    for s in kconfig_symbols(text) {
        out.push((SymbolKind::Kconfig, s));
    }
    for s in sysctl_procnames(text) {
        out.push((SymbolKind::Sysctl, s));
    }
    for s in file_scope_definitions(text) {
        out.push((SymbolKind::Definition, s));
    }
    out
}

/// Scan enabled user patches against the bundled set for duplicated
/// declarations. Empty result means nothing to report.
///
/// Disabled patches (`*.mypatch.disabled`) are skipped — they are not applied,
/// so they cannot collide.
pub fn scan(
    linux_tkg_path: &Path,
    kernel_series: &str,
    cfg: &BTreeMap<String, String>,
) -> Vec<Collision> {
    let user_dir = crate::core::patch_manager::get_patch_dir(linux_tkg_path, kernel_series);
    let bundled_dir = bundled_patch_dir(linux_tkg_path, kernel_series);

    // Bundled declarations, indexed by (kind, symbol).
    let mut bundled: BTreeMap<(SymbolKind, String), Vec<String>> = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(&bundled_dir) {
        for e in entries.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.ends_with(".patch") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(e.path()) {
                for (kind, sym) in symbols_of(&text) {
                    bundled.entry((kind, sym)).or_default().push(name.clone());
                }
            }
        }
    }
    if bundled.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&user_dir) {
        let mut names: Vec<_> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".mypatch") || n.ends_with(".patch"))
            })
            .collect();
        names.sort();
        for path in names {
            let uname = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (kind, sym) in symbols_of(&text) {
                if let Some(bundled_files) = bundled.get(&(kind, sym.clone())) {
                    for bf in bundled_files {
                        out.push(Collision {
                            kind,
                            symbol: sym.clone(),
                            user_patch: uname.clone(),
                            bundled_patch: bf.clone(),
                            bundled_applies: bundled_applies(bf, cfg),
                        });
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // The real patch that broke a 7.2 build, reduced to its essentials.
    const ARCH_USERNS: &str = r#"From 4580fa24 Mon Sep 17 00:00:00 2001
Subject: [PATCH] add sysctl to allow disabling unprivileged CLONE_NEWUSER
---
 kernel/fork.c | 24 ++++++++++++++++++++++++

diff --git a/kernel/fork.c b/kernel/fork.c
--- a/kernel/fork.c
+++ b/kernel/fork.c
@@ -127,6 +127,12 @@

 #include <kunit/visibility.h>

+#ifdef CONFIG_USER_NS
+static int unprivileged_userns_clone = 1;
+#else
+#define unprivileged_userns_clone 1
+#endif
+
 /*
"#;

    const TKG_USERNS: &str = r#"diff --git a/init/Kconfig b/init/Kconfig
--- a/init/Kconfig
+++ b/init/Kconfig
@@ -1,1 +1,8 @@
+config USER_NS_UNPRIVILEGED
+	bool "Allow unprivileged users to create namespaces"
diff --git a/kernel/sysctl.c b/kernel/sysctl.c
--- a/kernel/sysctl.c
+++ b/kernel/sysctl.c
@@ -1,1 +1,4 @@
+	{
+		.procname	= "unprivileged_userns_clone",
+		.data		= &unprivileged_userns_clone,
+	},
diff --git a/kernel/user_namespace.c b/kernel/user_namespace.c
--- a/kernel/user_namespace.c
+++ b/kernel/user_namespace.c
@@ -1,1 +1,4 @@
+#ifdef CONFIG_USER_NS_UNPRIVILEGED
+int unprivileged_userns_clone = 1;
+#endif
"#;

    const CACHY_O3: &str = r#"diff --git a/init/Kconfig b/init/Kconfig
--- a/init/Kconfig
+++ b/init/Kconfig
@@ -1,1 +1,4 @@
+config CC_OPTIMIZE_FOR_PERFORMANCE_O3
+	bool "Optimize more for performance (-O3)"
"#;

    #[test]
    fn finds_the_static_definition_that_breaks_fork_c() {
        let defs = file_scope_definitions(ARCH_USERNS);
        assert!(
            defs.contains("unprivileged_userns_clone"),
            "must see the column-0 static definition, got {defs:?}"
        );
    }

    #[test]
    fn the_bundled_patch_declares_the_same_symbol_three_ways() {
        assert!(file_scope_definitions(TKG_USERNS).contains("unprivileged_userns_clone"));
        assert!(sysctl_procnames(TKG_USERNS).contains("unprivileged_userns_clone"));
        assert!(kconfig_symbols(TKG_USERNS).contains("USER_NS_UNPRIVILEGED"));
    }

    #[test]
    fn kconfig_symbols_are_picked_up() {
        assert!(kconfig_symbols(CACHY_O3).contains("CC_OPTIMIZE_FOR_PERFORMANCE_O3"));
    }

    /// Indented declarations are locals and must not be reported — without this
    /// a large series produces constant false positives.
    #[test]
    fn locals_and_functions_are_not_file_scope_definitions() {
        let p = "diff --git a/x.c b/x.c\n+++ b/x.c\n@@ -1 +1,6 @@\n\
                 +	int local_thing = 1;\n\
                 +		u64 deeper = 2;\n\
                 +int some_function(int a)\n\
                 +static void another_fn(void)\n\
                 +#define NOT_A_DEF 1\n";
        let defs = file_scope_definitions(p);
        assert!(defs.is_empty(), "expected nothing, got {defs:?}");
    }

    #[test]
    fn a_plus_plus_plus_header_is_not_added_content() {
        // "+++ b/kernel/fork.c" must not be read as an added line.
        let p = "+++ b/kernel/fork.c\n@@ -1 +1,2 @@\n+int real_def;\n";
        assert_eq!(
            file_scope_definitions(p),
            ["real_def".to_string()].into_iter().collect()
        );
    }

    // ── applicability gating ────────────────────────────────────────────────

    fn cfg(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_userns_patch_is_active_unless_hardened_plus_cfs() {
        let f = "0001-add-sysctl-to-disallow-unprivileged-CLONE_NEWUSER-by.patch";
        // The reported case: bore, so the Arch patch branch is taken.
        assert_eq!(
            bundled_applies(f, &cfg(&[("_cpusched", "bore")])),
            Applicability::Applied
        );
        assert_eq!(
            bundled_applies(
                f,
                &cfg(&[
                    ("_cpusched", "cfs"),
                    ("_configfile", "config_hardened.x86_64")
                ])
            ),
            Applicability::NotApplied
        );
    }

    #[test]
    fn the_o3_patch_is_active_only_at_optlevel_two() {
        let f = "0013-optimize_harder_O3.patch";
        assert_eq!(
            bundled_applies(f, &cfg(&[("_compileroptlevel", "2")])),
            Applicability::Applied
        );
        assert_eq!(
            bundled_applies(f, &cfg(&[("_compileroptlevel", "1")])),
            Applicability::NotApplied
        );
        assert_eq!(bundled_applies(f, &cfg(&[])), Applicability::Unknown);
    }

    #[test]
    fn bore_gating_follows_cpusched() {
        assert_eq!(
            bundled_applies("0001-bore.patch", &cfg(&[("_cpusched", "bore")])),
            Applicability::Applied
        );
        assert_eq!(
            bundled_applies("0001-bore.patch", &cfg(&[("_cpusched", "eevdf")])),
            Applicability::NotApplied
        );
    }

    #[test]
    fn an_unrecognised_bundled_patch_is_unknown_not_asserted() {
        assert_eq!(
            bundled_applies("0099-something-new.patch", &cfg(&[])),
            Applicability::Unknown
        );
    }

    // ── end-to-end over a temp tree ─────────────────────────────────────────

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tree(tag: &str) -> Tmp {
        let d =
            std::env::temp_dir().join(format!("tkg-gui-conflicts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("linux-tkg-patches/7.2")).unwrap();
        std::fs::create_dir_all(d.join("linux72-tkg-userpatches")).unwrap();
        std::fs::write(
            d.join("linux-tkg-patches/7.2")
                .join("0001-add-sysctl-to-disallow-unprivileged-CLONE_NEWUSER-by.patch"),
            TKG_USERNS,
        )
        .unwrap();
        std::fs::write(
            d.join("linux-tkg-patches/7.2")
                .join("0013-optimize_harder_O3.patch"),
            CACHY_O3,
        )
        .unwrap();
        Tmp(d)
    }

    #[test]
    fn scan_reports_both_real_world_collisions() {
        let t = tree("both");
        let up = t.0.join("linux72-tkg-userpatches");
        std::fs::write(
            up.join("arch-patches-0001-add-sysctl-to-allow-disabling.mypatch"),
            ARCH_USERNS,
        )
        .unwrap();
        std::fs::write(
            up.join("kbuild-cachyos-patches-0001-Cachy-Allow-O3.mypatch"),
            CACHY_O3,
        )
        .unwrap();

        let c = scan(
            &t.0,
            "7.2",
            &cfg(&[("_cpusched", "bore"), ("_compileroptlevel", "2")]),
        );
        let syms: BTreeSet<&str> = c.iter().map(|x| x.symbol.as_str()).collect();
        assert!(
            syms.contains("unprivileged_userns_clone"),
            "missed the fork.c collision: {c:?}"
        );
        assert!(
            syms.contains("CC_OPTIMIZE_FOR_PERFORMANCE_O3"),
            "missed the O3 Kconfig collision: {c:?}"
        );
        assert!(c
            .iter()
            .all(|x| x.bundled_applies == Applicability::Applied));
    }

    #[test]
    fn the_o3_collision_is_latent_at_optlevel_two_disabled() {
        let t = tree("latent");
        std::fs::write(
            t.0.join("linux72-tkg-userpatches")
                .join("kbuild-cachyos-patches-0001-Cachy-Allow-O3.mypatch"),
            CACHY_O3,
        )
        .unwrap();
        let c = scan(&t.0, "7.2", &cfg(&[("_compileroptlevel", "1")]));
        assert_eq!(c.len(), 1);
        let g = group(&c);
        assert_eq!(g.len(), 1);
        assert!(!g[0].is_live(), "should be reported as not active: {c:?}");
    }

    #[test]
    fn a_disabled_user_patch_cannot_collide() {
        let t = tree("disabled");
        std::fs::write(
            t.0.join("linux72-tkg-userpatches")
                .join("arch-patches-0001.mypatch.disabled"),
            ARCH_USERNS,
        )
        .unwrap();
        assert!(scan(&t.0, "7.2", &cfg(&[])).is_empty());
    }

    #[test]
    fn a_clean_userpatch_set_reports_nothing() {
        let t = tree("clean");
        std::fs::write(
            t.0.join("linux72-tkg-userpatches")
                .join("handheld-0001-some-driver-quirk.mypatch"),
            "diff --git a/drivers/x.c b/drivers/x.c\n+++ b/drivers/x.c\n@@ -1 +1,2 @@\n+\tint quirk = 1;\n",
        )
        .unwrap();
        assert!(scan(&t.0, "7.2", &cfg(&[])).is_empty());
    }

    /// Run the scanner over a real linux-tkg checkout, for validating against a
    /// live patch set rather than fixtures. Ignored by default since it needs a
    /// tree on disk:
    ///
    /// ```text
    /// TKG_GUI_CONFLICT_TREE=/path/to/linux-tkg \
    ///   cargo test scan_a_real_tree -- --ignored --nocapture
    /// ```
    /// Scan a real tree, then actually apply the fix and re-scan, to prove the
    /// fix resolves what the scan found. Mutates the tree, so point it at a copy:
    ///
    /// ```text
    /// TKG_GUI_CONFLICT_TREE=/path/to/copy \
    ///   cargo test fix_a_real_tree -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "mutates the tree at TKG_GUI_CONFLICT_TREE"]
    fn fix_a_real_tree() {
        let Ok(root) = std::env::var("TKG_GUI_CONFLICT_TREE") else {
            return;
        };
        let root = PathBuf::from(root);
        let cfgmap = read_cfg(&root);
        let series = resolve_series(&root, &cfgmap).expect("could not resolve kernel series");

        let before = group(&scan(&root, &series, &cfgmap));
        let names = offending_user_patches(&before, true);
        println!(
            "before: {} finding(s); offending files: {names:?}",
            before.len()
        );
        assert!(!names.is_empty(), "nothing to fix in this tree");

        for r in disable_user_patches(&root, &series, &names) {
            println!("  {}", r.summary());
            assert!(!r.is_failure(), "{r:?}");
        }

        let after = group(&scan(&root, &series, &cfgmap));
        let live_after = after.iter().filter(|f| f.is_live()).count();
        println!("after: {} finding(s), {live_after} live", after.len());
        for f in &after {
            println!("  {}", f.headline());
        }
        assert_eq!(live_after, 0, "the fix must clear every live finding");
    }

    fn read_cfg(root: &Path) -> BTreeMap<String, String> {
        std::fs::read_to_string(root.join("customization.cfg"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches('"').to_string()))
            .collect()
    }

    #[test]
    #[ignore = "needs a real linux-tkg tree via TKG_GUI_CONFLICT_TREE"]
    fn scan_a_real_tree() {
        let Ok(root) = std::env::var("TKG_GUI_CONFLICT_TREE") else {
            return;
        };
        let root = PathBuf::from(root);
        let cfgmap: BTreeMap<String, String> =
            std::fs::read_to_string(root.join("customization.cfg"))
                .unwrap_or_default()
                .lines()
                .filter_map(|l| l.split_once('='))
                .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches('"').to_string()))
                .collect();
        let series = resolve_series(&root, &cfgmap).expect("could not resolve kernel series");
        let found = group(&scan(&root, &series, &cfgmap));
        let live = found.iter().filter(|f| f.is_live()).count();
        println!("series {series}: {} finding(s), {live} live", found.len());
        for f in &found {
            println!("  {}", f.headline());
            println!("      duplicated: {}", f.detail());
        }
    }

    // ── auto-resolution ─────────────────────────────────────────────────────

    #[test]
    fn only_live_findings_are_offered_for_removal_by_default() {
        let t = tree("offending");
        let up = t.0.join("linux72-tkg-userpatches");
        std::fs::write(up.join("arch-userns.mypatch"), ARCH_USERNS).unwrap();
        std::fs::write(up.join("cachy-o3.mypatch"), CACHY_O3).unwrap();

        // O3 bundled patch inactive at optlevel 1, so only the userns one is live.
        let g = group(&scan(
            &t.0,
            "7.2",
            &cfg(&[("_cpusched", "bore"), ("_compileroptlevel", "1")]),
        ));
        let live = offending_user_patches(&g, true);
        assert_eq!(live, vec!["arch-userns.mypatch".to_string()]);

        let all = offending_user_patches(&g, false);
        assert!(all.contains(&"cachy-o3.mypatch".to_string()));
        assert_eq!(all.len(), 2);
    }

    /// One userpatch can collide against several bundled patches at once — the
    /// real case collides with both the Arch userns patch and linux-hardened,
    /// which carries the same hunks. It must still be named once, because the
    /// fix is one file.
    #[test]
    fn a_user_patch_is_listed_once_even_when_it_collides_several_ways() {
        let t = tree("dedupe");
        // linux-hardened also carries the userns sysctl, as it does upstream.
        std::fs::write(
            t.0.join("linux-tkg-patches/7.2")
                .join("0012-linux-hardened.patch"),
            TKG_USERNS,
        )
        .unwrap();
        std::fs::write(
            t.0.join("linux72-tkg-userpatches")
                .join("arch-userns.mypatch"),
            ARCH_USERNS,
        )
        .unwrap();

        let g = group(&scan(&t.0, "7.2", &cfg(&[("_cpusched", "bore")])));
        assert!(
            g.len() > 1,
            "one userpatch should collide with both bundled patches, got {g:?}"
        );
        assert_eq!(
            offending_user_patches(&g, false),
            vec!["arch-userns.mypatch".to_string()],
            "the offending file must be named once, not per finding"
        );
        // Only the active bundled patch makes it live; hardened is not selected
        // under _cpusched=bore.
        assert_eq!(offending_user_patches(&g, true).len(), 1);
    }

    #[test]
    fn disabling_renames_and_makes_the_next_scan_clean() {
        let t = tree("disable");
        let up = t.0.join("linux72-tkg-userpatches");
        std::fs::write(up.join("arch-userns.mypatch"), ARCH_USERNS).unwrap();
        let c = cfg(&[("_cpusched", "bore")]);

        let g = group(&scan(&t.0, "7.2", &c));
        let names = offending_user_patches(&g, true);
        let res = disable_user_patches(&t.0, "7.2", &names);

        assert_eq!(res.len(), 1);
        assert!(!res[0].is_failure(), "{res:?}");
        assert_eq!(res[0].disabled_name(), Some("arch-userns.mypatch"));
        assert!(
            !up.join("arch-userns.mypatch").exists(),
            "original must be gone"
        );
        assert!(
            up.join("arch-userns.mypatch.disabled").is_file(),
            "must be renamed, not deleted -- the fix has to be undoable"
        );
        assert!(
            scan(&t.0, "7.2", &c).is_empty(),
            "re-scan after the fix must be clean"
        );
    }

    /// The rename must not clobber an existing `.disabled` file — that would
    /// destroy a patch the user had already set aside.
    #[test]
    fn an_existing_disabled_file_is_never_overwritten() {
        let t = tree("noclobber");
        let up = t.0.join("linux72-tkg-userpatches");
        std::fs::write(up.join("dup.mypatch"), ARCH_USERNS).unwrap();
        std::fs::write(up.join("dup.mypatch.disabled"), "PRECIOUS").unwrap();

        let res = disable_user_patches(&t.0, "7.2", &["dup.mypatch".to_string()]);
        assert!(matches!(res[0], Resolution::Skipped { .. }), "{res:?}");
        assert_eq!(
            std::fs::read_to_string(up.join("dup.mypatch.disabled")).unwrap(),
            "PRECIOUS"
        );
        assert!(up.join("dup.mypatch").is_file(), "original left in place");
    }

    #[test]
    fn path_traversal_in_a_name_is_refused() {
        let t = tree("traversal");
        for bad in ["../customization.cfg", "a/b.mypatch", ""] {
            let res = disable_user_patches(&t.0, "7.2", &[bad.to_string()]);
            assert!(
                matches!(res[0], Resolution::Skipped { .. }),
                "{bad:?} should be refused, got {res:?}"
            );
        }
        assert!(
            t.0.join("customization.cfg").exists() || !t.0.join("customization.cfg").exists(),
            "traversal must not have touched anything outside the patch dir"
        );
    }

    #[test]
    fn a_missing_file_is_skipped_not_an_error() {
        let t = tree("missing");
        let res = disable_user_patches(&t.0, "7.2", &["not-there.mypatch".to_string()]);
        assert!(matches!(res[0], Resolution::Skipped { .. }), "{res:?}");
        assert!(!res[0].is_failure());
    }

    #[test]
    fn autofix_is_off_unless_explicitly_enabled() {
        assert!(!autofix_enabled(&cfg(&[])));
        assert!(!autofix_enabled(&cfg(&[(AUTOFIX_KEY, "false")])));
        assert!(autofix_enabled(&cfg(&[(AUTOFIX_KEY, "true")])));
        assert!(autofix_enabled(&cfg(&[(AUTOFIX_KEY, "yes")])));
    }

    #[test]
    fn series_resolves_from_version_then_from_the_userpatch_dir() {
        let t = tree("series");
        assert_eq!(
            resolve_series(&t.0, &cfg(&[("_version", "7.2")])).as_deref(),
            Some("7.2")
        );
        // No _version: derived from linux72-tkg-userpatches.
        assert_eq!(resolve_series(&t.0, &cfg(&[])).as_deref(), Some("7.2"));
        // A _version with no bundled dir falls back rather than guessing.
        assert_eq!(
            resolve_series(&t.0, &cfg(&[("_version", "9.9")])).as_deref(),
            Some("7.2")
        );
    }
}
