//! Award Data Archive Full files for the current fiscal year and the one before it.
//! Delta files are not read.

use std::fs::{self, File};
use std::io::{copy, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::Value;

const LIST_URL: &str = "https://api.usaspending.gov/api/v2/bulk_download/list_monthly_files/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveFile {
    pub file_name: String,
    pub url: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedFile {
    pub file_name: String,
    pub url: String,
    pub fiscal_year: Option<i64>,
}

pub fn select_full(files: &[ListedFile], prior_fy: i32, current_fy: i32) -> Vec<ListedFile> {
    let prior = format!("FY{prior_fy}_");
    let current = format!("FY{current_fy}_");
    let mut out: Vec<ListedFile> = files
        .iter()
        .filter(|file| {
            file.file_name.contains("_Full_")
                && (file.file_name.starts_with(&prior) || file.file_name.starts_with(&current))
        })
        .cloned()
        .collect();
    out.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    out.dedup_by(|a, b| a.file_name == b.file_name);
    out
}

pub fn download_full_files(
    cache_dir: &Path,
    prior_fy: i32,
    current_fy: i32,
) -> Result<Vec<ArchiveFile>> {
    tracing::info!(prior_fy, current_fy, "listing award archive");
    let listed = list_archive(prior_fy, current_fy)?;
    let selected = select_full(&listed, prior_fy, current_fy);
    fs::create_dir_all(cache_dir).with_context(|| format!("create {}", cache_dir.display()))?;
    let mut out = Vec::new();
    for file in selected {
        let path = cache_dir.join(&file.file_name);
        download(&file.url, &path)?;
        out.push(ArchiveFile {
            file_name: file.file_name,
            url: file.url,
            path,
        });
    }
    Ok(out)
}

fn list_archive(prior_fy: i32, current_fy: i32) -> Result<Vec<ListedFile>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("http client")?;
    let mut files = Vec::new();
    for fiscal_year in [prior_fy, current_fy] {
        for kind in ["contracts", "assistance"] {
            let body = serde_json::json!({
                "agency": "all",
                "fiscal_year": fiscal_year,
                "type": kind
            });
            let text = client
                .post(LIST_URL)
                .json(&body)
                .send()
                .context("list monthly files")?
                .error_for_status()
                .context("list monthly files status")?
                .text()
                .context("list monthly files body")?;
            files.extend(parse_list(&text)?);
        }
    }
    Ok(files)
}

fn parse_list(body: &str) -> Result<Vec<ListedFile>> {
    let value: Value = serde_json::from_str(body).context("list json")?;
    let Some(rows) = value.get("monthly_files").and_then(|v| v.as_array()) else {
        bail!("monthly file list has no monthly_files");
    };
    let mut out = Vec::new();
    for row in rows {
        let file_name = row
            .get("file_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let url = row
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if file_name.is_empty() || url.is_empty() {
            continue;
        }
        let fiscal_year = row.get("fiscal_year").and_then(|v| v.as_i64());
        out.push(ListedFile {
            file_name,
            url,
            fiscal_year,
        });
    }
    Ok(out)
}

fn download(url: &str, dest: &Path) -> Result<()> {
    if zip_opens(dest) {
        let len = fs::metadata(dest)?.len();
        tracing::info!(file = %dest.display(), bytes = len, "archive cached");
        return Ok(());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(7200))
        .build()
        .context("http client")?;
    let head = client
        .head(url)
        .send()
        .with_context(|| format!("HEAD {url}"))?
        .error_for_status()
        .with_context(|| format!("HEAD {url}"))?;
    let expected = head.content_length().filter(|n| *n > 0);
    let have = if dest.is_file() {
        fs::metadata(dest)?.len()
    } else {
        0
    };
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let appending = expected.is_some_and(|n| have > 0 && have < n);
    let mut request = client.get(url);
    if appending {
        request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let mut response = request.send().with_context(|| format!("GET {url}"))?;
    let resume = appending && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if !resume {
        response = response
            .error_for_status()
            .with_context(|| format!("GET {url}"))?;
    }
    let mut file = if resume {
        std::fs::OpenOptions::new()
            .append(true)
            .open(dest)
            .with_context(|| format!("append {}", dest.display()))?
    } else {
        let partial = dest.with_extension("partial");
        File::create(&partial).with_context(|| format!("create {}", partial.display()))?
    };
    let writing_partial = !resume;
    copy(&mut response, &mut file).with_context(|| format!("write {}", dest.display()))?;
    file.flush()?;
    drop(file);
    if writing_partial {
        let partial = dest.with_extension("partial");
        fs::rename(&partial, dest)?;
    }
    if !zip_opens(dest) {
        let len = fs::metadata(dest)?.len();
        bail!("GET {url} wrote {len} bytes and the zip does not open");
    }
    let len = fs::metadata(dest)?.len();
    if expected.is_some_and(|n| n != len) {
        tracing::warn!(
            file = %dest.display(),
            bytes = len,
            expected,
            "archive length differs from HEAD; the zip opens, so it is kept"
        );
    }
    tracing::info!(file = %dest.display(), bytes = len, "archive downloaded");
    Ok(())
}

fn zip_opens(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    zip::ZipArchive::new(file).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_files_skip_the_all_years_delta() {
        let files = vec![
            ListedFile {
                file_name: "FY2026_All_Contracts_Full_20260906.zip".into(),
                url: "https://example.invalid/c".into(),
                fiscal_year: Some(2026),
            },
            ListedFile {
                file_name: "FY(All)_All_Contracts_Delta_20260924.zip".into(),
                url: "https://example.invalid/d".into(),
                fiscal_year: None,
            },
            ListedFile {
                file_name: "FY2026_All_Contracts_Full_20260906.zip".into(),
                url: "https://example.invalid/c2".into(),
                fiscal_year: Some(2026),
            },
            ListedFile {
                file_name: "FY2027_All_Assistance_Full_20261008.zip".into(),
                url: "https://example.invalid/a".into(),
                fiscal_year: Some(2027),
            },
        ];
        let selected = select_full(&files, 2026, 2027);
        assert_eq!(
            selected
                .iter()
                .map(|f| f.file_name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "FY2026_All_Contracts_Full_20260906.zip",
                "FY2027_All_Assistance_Full_20261008.zip"
            ]
        );
    }

    #[test]
    fn a_later_october_drops_the_year_that_fell_behind() {
        let files = vec![
            ListedFile {
                file_name: "FY2026_All_Contracts_Full_20260906.zip".into(),
                url: "https://example.invalid/old".into(),
                fiscal_year: Some(2026),
            },
            ListedFile {
                file_name: "FY2027_All_Contracts_Full_20271001.zip".into(),
                url: "https://example.invalid/c".into(),
                fiscal_year: Some(2027),
            },
            ListedFile {
                file_name: "FY2028_All_Assistance_Full_20271001.zip".into(),
                url: "https://example.invalid/a".into(),
                fiscal_year: Some(2028),
            },
            ListedFile {
                file_name: "FY(All)_All_Contracts_Delta_20271001.zip".into(),
                url: "https://example.invalid/d".into(),
                fiscal_year: None,
            },
        ];
        let selected = select_full(&files, 2027, 2028);
        assert_eq!(
            selected
                .iter()
                .map(|f| f.file_name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "FY2027_All_Contracts_Full_20271001.zip",
                "FY2028_All_Assistance_Full_20271001.zip"
            ]
        );
    }
}
