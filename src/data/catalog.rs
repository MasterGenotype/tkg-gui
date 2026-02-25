/// A catalog entry describing a well-known userpatch source
#[derive(Clone, Debug)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// URL template with {series} placeholder
    pub url_template: &'static str,
    /// Filename template with {series} placeholder
    pub filename_template: &'static str,
    /// Supported kernel series (e.g., ["6.12", "6.13"])
    pub supported_series: &'static [&'static str],
}

impl CatalogEntry {
    /// Get the URL for a specific kernel series
    pub fn url_for_series(&self, series: &str) -> String {
        self.url_template.replace("{series}", series)
    }

    /// Get the filename for a specific kernel series
    pub fn filename_for_series(&self, series: &str) -> String {
        self.filename_template.replace("{series}", series)
    }

    /// Check if this entry supports the given kernel series
    pub fn supports_series(&self, series: &str) -> bool {
        self.supported_series.contains(&series)
    }
}

/// Filter catalog to entries supporting the given kernel series
pub fn catalog_for_series(series: &str) -> Vec<&'static CatalogEntry> {
    CATALOG
        .iter()
        .filter(|e| e.supports_series(series))
        .collect()
}

static CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "acs-override",
        name: "ACS Override Patch",
        description: "Allows IOMMU groups to be split for better VFIO passthrough",
        url_template: "https://raw.githubusercontent.com/benbaker76/linux-acs-override/main/workspaces/{series}/acso.patch",
        filename_template: "acs-override-{series}.patch",
        supported_series: &["6.10", "6.11", "6.12", "6.13"],
    },
    CatalogEntry {
        id: "bbr3",
        name: "BBRv3 TCP Congestion Control",
        description: "Google's BBRv3 TCP congestion control algorithm",
        url_template: "https://raw.githubusercontent.com/CachyOS/kernel-patches/master/{series}/misc/0001-bbr3.patch",
        filename_template: "bbr3-{series}.patch",
        supported_series: &["6.11", "6.12", "6.13"],
    },
    CatalogEntry {
        id: "cachy-fixes",
        name: "CachyOS Kernel Fixes",
        description: "Collection of kernel fixes from CachyOS",
        url_template: "https://raw.githubusercontent.com/CachyOS/kernel-patches/master/{series}/all/0001-cachyos-base-all.patch",
        filename_template: "cachy-fixes-{series}.patch",
        supported_series: &["6.11", "6.12", "6.13"],
    },
    CatalogEntry {
        id: "graysky-cpu",
        name: "Graysky CPU Optimizations",
        description: "Additional CPU compiler optimizations by graysky2",
        url_template: "https://raw.githubusercontent.com/graysky2/kernel_compiler_patch/master/more-uarches-for-kernel-6.8-rc4%2B.patch",
        filename_template: "graysky-cpu-{series}.patch",
        supported_series: &["6.8", "6.9", "6.10", "6.11", "6.12", "6.13"],
    },
    CatalogEntry {
        id: "futex-waitv",
        name: "Futex2/waitv Backport",
        description: "Backport of futex2 waitv for Steam/Proton compatibility",
        url_template: "https://raw.githubusercontent.com/CachyOS/kernel-patches/master/{series}/misc/0001-futex-Add-entry-point-for-FUTEX_WAIT_MULTIPLE.patch",
        filename_template: "futex-waitv-{series}.patch",
        supported_series: &["6.10", "6.11"],
    },
    CatalogEntry {
        id: "zstd-upstream",
        name: "ZSTD Upstream Updates",
        description: "Latest upstream ZSTD compression improvements",
        url_template: "https://raw.githubusercontent.com/CachyOS/kernel-patches/master/{series}/misc/0001-zstd.patch",
        filename_template: "zstd-upstream-{series}.patch",
        supported_series: &["6.11", "6.12", "6.13"],
    },
    CatalogEntry {
        id: "amd-pstate",
        name: "AMD P-State Improvements",
        description: "Enhanced AMD P-State driver patches",
        url_template: "https://raw.githubusercontent.com/CachyOS/kernel-patches/master/{series}/misc/0001-amd-pstate.patch",
        filename_template: "amd-pstate-{series}.patch",
        supported_series: &["6.11", "6.12", "6.13"],
    },
    CatalogEntry {
        id: "le9",
        name: "le9 OOM Protection",
        description: "Protect the working set under memory pressure",
        url_template: "https://raw.githubusercontent.com/CachyOS/kernel-patches/master/{series}/misc/0001-mm-add-le9.patch",
        filename_template: "le9-{series}.patch",
        supported_series: &["6.10", "6.11", "6.12"],
    },
];
