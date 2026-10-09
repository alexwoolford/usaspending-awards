use std::ops::{Deref, DerefMut};
use std::path::Path;

use anyhow::{Context, Result};
use capturable_state::{
    apply_runtime_pragmas, install, CaptureConfig, CaptureMode, Nudge, TableSpec,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::parse::AwardRow;
use crate::time::utc_iso;

pub const DB_NAME: &str = "usaspending-awards";

pub struct WorkDb {
    conn: Connection,
    pub nudge: Nudge,
}

impl Deref for WorkDb {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.conn
    }
}

impl DerefMut for WorkDb {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

fn open_conn(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
    }
    let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    apply_runtime_pragmas(&conn)?;
    conn.execute_batch(include_str!("schema.sql"))
        .context("apply schema")?;
    Ok(conn)
}

pub fn open(path: &Path) -> Result<Connection> {
    open_conn(path)
}

pub fn open_work(path: &Path) -> Result<WorkDb> {
    let conn = open_conn(path)?;
    let nudge = install(
        &conn,
        &CaptureConfig::new(
            DB_NAME,
            path,
            &[
                TableSpec::new("transactions", CaptureMode::After),
                TableSpec::new("ingest_runs", CaptureMode::After),
            ],
        ),
    )?;
    Ok(WorkDb { conn, nudge })
}

pub fn clear_stage(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM transaction_stage", [])
        .context("clear stage")?;
    Ok(())
}

pub fn stage_row(conn: &Connection, row: &AwardRow) -> Result<()> {
    conn.execute(
        "INSERT INTO transaction_stage (
            transaction_key, kind, uei, recipient_name, award_id, modification_number,
            agency, action_date, obligation
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(transaction_key) DO UPDATE SET
            kind = excluded.kind,
            uei = excluded.uei,
            recipient_name = excluded.recipient_name,
            award_id = excluded.award_id,
            modification_number = excluded.modification_number,
            agency = excluded.agency,
            action_date = excluded.action_date,
            obligation = excluded.obligation",
        params![
            row.transaction_key,
            row.kind,
            row.uei,
            row.recipient_name,
            row.award_id,
            row.modification_number,
            row.agency,
            row.action_date,
            row.obligation,
        ],
    )?;
    Ok(())
}

/// Change-aware promote. An identical live row emits no extra `_outbox` row.
pub fn promote_stage(conn: &Connection, as_of_date: &str) -> Result<i64> {
    let tx = conn
        .unchecked_transaction()
        .context("promote transaction")?;
    let mut stmt = tx.prepare(
        "SELECT transaction_key, kind, uei, recipient_name, award_id, modification_number,
                agency, action_date, obligation
         FROM transaction_stage",
    )?;
    let collected: Vec<AwardRow> = stmt
        .query_map([], |row| {
            Ok(AwardRow {
                transaction_key: row.get(0)?,
                kind: row.get(1)?,
                uei: row.get(2)?,
                recipient_name: row.get(3)?,
                award_id: row.get(4)?,
                modification_number: row.get(5)?,
                agency: row.get(6)?,
                action_date: row.get(7)?,
                obligation: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut upserted = 0i64;
    for row in collected {
        if upsert_transaction(&tx, &row, as_of_date)? {
            upserted += 1;
        }
    }
    tx.commit().context("commit promote")?;
    Ok(upserted)
}

fn upsert_transaction(conn: &Connection, row: &AwardRow, as_of_date: &str) -> Result<bool> {
    let n = conn.execute(
        "INSERT INTO transactions (
            transaction_key, kind, uei, recipient_name, award_id, modification_number,
            agency, action_date, obligation, as_of_date, deleted_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)
         ON CONFLICT(transaction_key) DO UPDATE SET
            kind = excluded.kind,
            uei = excluded.uei,
            recipient_name = excluded.recipient_name,
            award_id = excluded.award_id,
            modification_number = excluded.modification_number,
            agency = excluded.agency,
            action_date = excluded.action_date,
            obligation = excluded.obligation,
            as_of_date = excluded.as_of_date,
            deleted_at = NULL
         WHERE transactions.kind IS NOT excluded.kind
            OR transactions.uei IS NOT excluded.uei
            OR transactions.recipient_name IS NOT excluded.recipient_name
            OR transactions.award_id IS NOT excluded.award_id
            OR transactions.modification_number IS NOT excluded.modification_number
            OR transactions.agency IS NOT excluded.agency
            OR transactions.action_date IS NOT excluded.action_date
            OR transactions.obligation IS NOT excluded.obligation
            OR transactions.deleted_at IS NOT NULL",
        params![
            row.transaction_key,
            row.kind,
            row.uei,
            row.recipient_name,
            row.award_id,
            row.modification_number,
            row.agency,
            row.action_date,
            row.obligation,
            as_of_date,
        ],
    )?;
    Ok(n > 0)
}

pub fn retract_absent(conn: &Connection, deleted_at: i64) -> Result<i64> {
    let n = conn.execute(
        "UPDATE transactions
         SET deleted_at = ?1
         WHERE deleted_at IS NULL
           AND NOT EXISTS (
             SELECT 1 FROM transaction_stage s
             WHERE s.transaction_key = transactions.transaction_key
           )",
        params![deleted_at],
    )?;
    Ok(n as i64)
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_run(
    conn: &Connection,
    as_of_date: &str,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: chrono::DateTime<chrono::Utc>,
    status: &str,
    files_read: i64,
    rows_scanned: i64,
    transactions_upserted: i64,
    transactions_retracted: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO ingest_runs (
            as_of_date, started_at, finished_at, status,
            files_read, rows_scanned, transactions_upserted, transactions_retracted
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(as_of_date) DO UPDATE SET
            started_at = excluded.started_at,
            finished_at = excluded.finished_at,
            status = excluded.status,
            files_read = excluded.files_read,
            rows_scanned = excluded.rows_scanned,
            transactions_upserted = excluded.transactions_upserted,
            transactions_retracted = excluded.transactions_retracted",
        params![
            as_of_date,
            utc_iso(started_at),
            utc_iso(finished_at),
            status,
            files_read,
            rows_scanned,
            transactions_upserted,
            transactions_retracted,
        ],
    )?;
    Ok(())
}

#[derive(Debug)]
pub struct StatusRow {
    pub as_of_date: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String,
    pub files_read: i64,
    pub rows_scanned: i64,
    pub transactions_upserted: i64,
    pub transactions_retracted: i64,
}

pub fn last_run(conn: &Connection) -> Result<Option<StatusRow>> {
    conn.query_row(
        "SELECT as_of_date, started_at, finished_at, status,
                files_read, rows_scanned, transactions_upserted, transactions_retracted
         FROM ingest_runs
         ORDER BY as_of_date DESC
         LIMIT 1",
        [],
        |row| {
            Ok(StatusRow {
                as_of_date: row.get(0)?,
                started_at: row.get(1)?,
                finished_at: row.get(2)?,
                status: row.get(3)?,
                files_read: row.get(4)?,
                rows_scanned: row.get(5)?,
                transactions_upserted: row.get(6)?,
                transactions_retracted: row.get(7)?,
            })
        },
    )
    .optional()
    .context("last run")
}

#[derive(Debug)]
pub struct LookupRow {
    pub transaction_key: String,
    pub kind: String,
    pub uei: String,
    pub recipient_name: String,
    pub award_id: String,
    pub agency: String,
    pub action_date: String,
    pub obligation: String,
}

pub fn lookup(conn: &Connection, uei: &str) -> Result<Vec<LookupRow>> {
    let uei = uei.trim().to_ascii_uppercase();
    let mut stmt = conn.prepare(
        "SELECT transaction_key, kind, uei, recipient_name, award_id, agency, action_date, obligation
         FROM transactions
         WHERE deleted_at IS NULL AND uei = ?1
         ORDER BY action_date, transaction_key
         LIMIT 20",
    )?;
    let rows = stmt.query_map(params![uei], |row| {
        Ok(LookupRow {
            transaction_key: row.get(0)?,
            kind: row.get(1)?,
            uei: row.get(2)?,
            recipient_name: row.get(3)?,
            award_id: row.get(4)?,
            agency: row.get(5)?,
            action_date: row.get(6)?,
            obligation: row.get(7)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn live_count(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM transactions WHERE deleted_at IS NULL",
        [],
        |row| row.get(0),
    )
    .context("live count")
}

pub fn outbox_count(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM _outbox", [], |row| row.get(0))
        .context("outbox count")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn row(key: &str) -> AwardRow {
        AwardRow {
            transaction_key: key.into(),
            kind: "contract".into(),
            uei: "MJENSFHZJP11".into(),
            recipient_name: "LOCKHEED MARTIN CORPORATION".into(),
            award_id: "PIID1".into(),
            modification_number: "0".into(),
            agency: "Department of Defense".into(),
            action_date: "2026-03-01".into(),
            obligation: "10".into(),
        }
    }

    #[test]
    fn identical_rerun_writes_no_outbox_row_and_retraction_does() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("awards.sqlite");
        let db = open_work(&path).unwrap();
        clear_stage(&db).unwrap();
        stage_row(&db, &row("k1")).unwrap();
        stage_row(&db, &row("k2")).unwrap();
        let upserted = promote_stage(&db, "2026-10-08").unwrap();
        assert_eq!(upserted, 2);
        let after_insert = outbox_count(&db).unwrap();
        clear_stage(&db).unwrap();
        stage_row(&db, &row("k1")).unwrap();
        stage_row(&db, &row("k2")).unwrap();
        assert_eq!(promote_stage(&db, "2026-10-09").unwrap(), 0);
        assert_eq!(outbox_count(&db).unwrap(), after_insert);
        clear_stage(&db).unwrap();
        stage_row(&db, &row("k1")).unwrap();
        let day = Utc.with_ymd_and_hms(2026, 10, 8, 12, 0, 0).unwrap();
        assert_eq!(retract_absent(&db, day.timestamp()).unwrap(), 1);
        assert_eq!(live_count(&db).unwrap(), 1);
    }
}
