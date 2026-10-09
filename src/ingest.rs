//! Scan every Full file into a stage table, then promote.
//! A failure leaves the live table unretracted.

use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use zip::ZipArchive;

use crate::archive::{download_full_files, ArchiveFile};
use crate::db::{clear_stage, promote_stage, retract_absent, stage_row, upsert_run};
use crate::parse::scan_csv;

pub struct IngestStats {
    pub status: &'static str,
    pub files_read: i64,
    pub rows_scanned: i64,
    pub transactions_upserted: i64,
    pub transactions_retracted: i64,
}

pub fn ingest_archives(
    conn: &Connection,
    ueis: &HashSet<String>,
    cache_dir: &Path,
    window_end: &str,
) -> Result<(i64, i64)> {
    let files = download_full_files(cache_dir)?;
    if files.is_empty() {
        bail!("award archive listed no FY2026 or FY2027 Full file");
    }
    clear_stage(conn)?;
    let mut scanned = 0i64;
    for file in &files {
        tracing::info!(file = %file.file_name, "reading archive");
        scanned += scan_zip(conn, ueis, file, window_end)?;
    }
    let kept: i64 = conn.query_row("SELECT COUNT(*) FROM transaction_stage", [], |row| {
        row.get(0)
    })?;
    if scanned > 0 && kept == 0 {
        clear_stage(conn)?;
        bail!("scanned {scanned} archive rows and kept none");
    }
    Ok((files.len() as i64, scanned))
}

fn scan_zip(
    conn: &Connection,
    ueis: &HashSet<String>,
    file: &ArchiveFile,
    window_end: &str,
) -> Result<i64> {
    let handle = File::open(&file.path).with_context(|| format!("open {}", file.path.display()))?;
    let mut archive = ZipArchive::new(handle).with_context(|| format!("zip {}", file.file_name))?;
    let tx = conn.unchecked_transaction().context("stage transaction")?;
    let mut scanned = 0i64;
    let mut saw_csv = false;
    for index in 0..archive.len() {
        let member = archive
            .by_index(index)
            .with_context(|| format!("zip member {index} in {}", file.file_name))?;
        if !member.name().to_ascii_lowercase().ends_with(".csv") {
            continue;
        }
        saw_csv = true;
        let name = member.name().to_string();
        tracing::info!(csv = %name, "scanning");
        let stats = scan_csv(member, ueis, window_end, |row| stage_row(&tx, &row))?;
        tracing::info!(csv = %name, scanned = stats.scanned, kept = stats.kept, "scanned");
        scanned += stats.scanned as i64;
    }
    if !saw_csv {
        bail!("{} contains no csv", file.file_name);
    }
    tx.commit().context("commit stage")?;
    Ok(scanned)
}

pub fn apply_rows(
    conn: &Connection,
    as_of_date: &str,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: chrono::DateTime<chrono::Utc>,
    files_read: i64,
    rows_scanned: i64,
) -> Result<IngestStats> {
    let upserted = promote_stage(conn, as_of_date)?;
    let retracted = retract_absent(conn, finished_at.timestamp())?;
    clear_stage(conn)?;
    upsert_run(
        conn,
        as_of_date,
        started_at,
        finished_at,
        "ok",
        files_read,
        rows_scanned,
        upserted,
        retracted,
    )?;
    Ok(IngestStats {
        status: "ok",
        files_read,
        rows_scanned,
        transactions_upserted: upserted,
        transactions_retracted: retracted,
    })
}

pub fn record_error(
    conn: &Connection,
    as_of_date: &str,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let _ = clear_stage(conn);
    upsert_run(
        conn,
        as_of_date,
        started_at,
        finished_at,
        "error",
        0,
        0,
        0,
        0,
    )?;
    Ok(())
}

pub fn finish_log(stats: &IngestStats, as_of: &str) {
    tracing::info!(
        as_of_date = %as_of,
        files = stats.files_read,
        scanned = stats.rows_scanned,
        upserted = stats.transactions_upserted,
        retracted = stats.transactions_retracted,
        "ingest finished"
    );
}
