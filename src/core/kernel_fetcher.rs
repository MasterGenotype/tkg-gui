use regex::Regex;
use scraper::{Html, Selector};

const KERNEL_TAGS_URL: &str =
    "https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/refs/tags";
const KERNEL_BASE_URL: &str =
    "https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git";

#[derive(Clone, Debug)]
pub struct VersionInfo {
    pub version: String,
    pub date: Option<String>,
}

pub enum FetchResult {
    Done(Vec<VersionInfo>),
    Error(String),
}

pub enum ShortlogResult {
    Done(Vec<CommitInfo>),
    Error(String),
}

#[derive(Clone, Debug)]
pub struct CommitInfo {
    pub hash: String,
    pub subject: String,
    pub author: String,
}

pub fn fetch_tags() -> FetchResult {
    match fetch_tags_inner() {
        Ok(tags) => FetchResult::Done(tags),
        Err(e) => FetchResult::Error(e),
    }
}

fn fetch_tags_inner() -> Result<Vec<VersionInfo>, String> {
    let response = ureq::get(KERNEL_TAGS_URL)
        .call()
        .map_err(|e| e.to_string())?;

    let body = response.into_string().map_err(|e| e.to_string())?;
    let document = Html::parse_document(&body);

    // cgit renders tags in table rows
    let row_selector = Selector::parse("tr").map_err(|e| format!("{:?}", e))?;
    let link_selector = Selector::parse("a").map_err(|e| format!("{:?}", e))?;
    let date_selector = Selector::parse("td:nth-child(3)").map_err(|e| format!("{:?}", e))?;
    let version_re = Regex::new(r"^v\d+\.\d+(\.\d+)?$").unwrap();

    let mut versions: Vec<VersionInfo> = Vec::new();

    for row in document.select(&row_selector) {
        // Find version link in this row
        if let Some(link) = row.select(&link_selector).next() {
            let text = link.text().collect::<String>();
            if version_re.is_match(&text) {
                // Try to find date in this row
                let date = row.select(&date_selector).next().map(|el| {
                    el.text().collect::<String>().trim().to_string()
                });
                versions.push(VersionInfo {
                    version: text,
                    date,
                });
            }
        }
    }

    // Sort by version number, newest first
    versions.sort_by(|a, b| compare_versions(&b.version, &a.version));
    versions.dedup_by(|a, b| a.version == b.version);

    Ok(versions)
}

/// Fetch shortlog (commit summaries) between two versions
pub fn fetch_shortlog(from_version: &str, to_version: &str) -> ShortlogResult {
    match fetch_shortlog_inner(from_version, to_version) {
        Ok(commits) => ShortlogResult::Done(commits),
        Err(e) => ShortlogResult::Error(e),
    }
}

fn fetch_shortlog_inner(from_version: &str, to_version: &str) -> Result<Vec<CommitInfo>, String> {
    // cgit URL for log between two tags
    // Format: /log/?id=v6.13.1&id2=v6.13
    let url = format!(
        "{}/log/?id={}&id2={}",
        KERNEL_BASE_URL, to_version, from_version
    );

    let response = ureq::get(&url).call().map_err(|e| e.to_string())?;
    let body = response.into_string().map_err(|e| e.to_string())?;
    let document = Html::parse_document(&body);

    // cgit log page structure:
    // <table class='list'><tr class='nohover'><th>...</th></tr>
    // <tr><td>date</td><td><a href='...?id=HASH'>subject</a></td><td>author</td>...</tr>
    let row_selector = Selector::parse("table.list tr").map_err(|e| format!("{:?}", e))?;
    let link_selector = Selector::parse("td:nth-child(2) a").map_err(|e| format!("{:?}", e))?;
    let author_selector = Selector::parse("td:nth-child(3)").map_err(|e| format!("{:?}", e))?;
    
    let mut commits: Vec<CommitInfo> = Vec::new();

    for row in document.select(&row_selector) {
        // Skip header row (contains <th> not <td>)
        if let Some(subject_el) = row.select(&link_selector).next() {
            let subject = subject_el.text().collect::<String>().trim().to_string();
            
            // Skip empty subjects (header row)
            if subject.is_empty() {
                continue;
            }
            
            // Extract commit hash from href: .../commit/?id=HASH
            let hash = subject_el
                .value()
                .attr("href")
                .and_then(|href| href.split("id=").nth(1))
                .map(|h| h.chars().take(12).collect())
                .unwrap_or_default();
            
            let author = row
                .select(&author_selector)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            commits.push(CommitInfo {
                hash,
                subject,
                author,
            });
        }
    }

    Ok(commits)
}

/// Get the previous version in the same series (e.g., v6.13.1 -> v6.13)
pub fn get_previous_version(version: &str, all_versions: &[VersionInfo]) -> Option<String> {
    let idx = all_versions.iter().position(|v| v.version == version)?;
    
    // Get major.minor of current version
    let current_parts: Vec<&str> = version.trim_start_matches('v').split('.').collect();
    if current_parts.len() < 2 {
        return None;
    }
    let current_major_minor = format!("{}.{}", current_parts[0], current_parts[1]);
    
    // Look for previous version in same series
    for v in all_versions.iter().skip(idx + 1) {
        let parts: Vec<&str> = v.version.trim_start_matches('v').split('.').collect();
        if parts.len() >= 2 {
            let major_minor = format!("{}.{}", parts[0], parts[1]);
            if major_minor == current_major_minor {
                return Some(v.version.clone());
            }
        }
    }
    
    // If no previous in same series, return the base version (e.g., v6.13)
    if current_parts.len() > 2 {
        let base = format!("v{}.{}", current_parts[0], current_parts[1]);
        if all_versions.iter().any(|v| v.version == base) {
            return Some(base);
        }
    }
    
    None
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let va = parse(a);
    let vb = parse(b);
    va.cmp(&vb)
}
