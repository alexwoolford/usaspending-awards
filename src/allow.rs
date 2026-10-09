//! Live UEIs from the exact map. Read-only. The name-and-state rule stays in uei-cik.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags};

pub fn load_ueis(path: &Path) -> Result<HashSet<String>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {}", path.display()))?;
    let mut stmt = conn
        .prepare("SELECT uei FROM matches WHERE deleted_at IS NULL")
        .context("read mapped UEIs")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = HashSet::new();
    for row in rows {
        let uei = row.context("uei row")?.trim().to_ascii_uppercase();
        if !uei.is_empty() {
            out.insert(uei);
        }
    }
    if out.is_empty() {
        bail!("uei map at {} has no live UEIs", path.display());
    }
    Ok(out)
}
