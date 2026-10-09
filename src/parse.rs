//! Keep a prime-award transaction when the recipient UEI is on the map.
//! `recipient_parent_uei` is never read.

use std::collections::HashSet;
use std::io::Read;

use anyhow::{bail, Context, Result};
use csv::StringRecord;

pub const KIND_CONTRACT: &str = "contract";
pub const KIND_GRANT: &str = "grant";

/// Block, formula, and project grants, plus cooperative agreements.
const GRANT_CODES: &[&str] = &["02", "03", "04", "05"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwardRow {
    pub transaction_key: String,
    pub kind: String,
    pub uei: String,
    pub recipient_name: String,
    pub award_id: String,
    pub modification_number: String,
    pub agency: String,
    pub action_date: String,
    pub obligation: String,
}

#[derive(Debug, Default, Clone)]
pub struct ScanStats {
    pub scanned: u64,
    pub kept: u64,
}

struct Cols {
    kind: &'static str,
    transaction_key: usize,
    award_id: usize,
    award_id_alt: Option<usize>,
    modification_number: usize,
    uei: usize,
    recipient_name: usize,
    agency: usize,
    action_date: usize,
    obligation: usize,
    assistance_type: Option<usize>,
}

pub fn scan_csv<R: Read>(
    reader: R,
    ueis: &HashSet<String>,
    window_start: &str,
    window_end: &str,
    mut on_row: impl FnMut(AwardRow) -> Result<()>,
) -> Result<ScanStats> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(reader);
    let headers = rdr.headers().context("csv header")?.clone();
    let cols = columns(&headers)?;
    let mut stats = ScanStats::default();
    let mut record = StringRecord::new();
    while rdr.read_record(&mut record).context("csv record")? {
        stats.scanned += 1;
        if let Some(row) = keep_record(&cols, &record, ueis, window_start, window_end) {
            on_row(row)?;
            stats.kept += 1;
        }
        if stats.scanned % 500_000 == 0 {
            tracing::info!(scanned = stats.scanned, kept = stats.kept, "archive rows");
        }
    }
    Ok(stats)
}

fn columns(headers: &StringRecord) -> Result<Cols> {
    let names: Vec<String> = headers
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let pos = |name: &str| names.iter().position(|h| h == name);
    let need = |name: &str| -> Result<usize> {
        pos(name).with_context(|| format!("csv is missing {name}"))
    };
    if pos("contract_transaction_unique_key").is_some() {
        Ok(Cols {
            kind: KIND_CONTRACT,
            transaction_key: need("contract_transaction_unique_key")?,
            award_id: need("award_id_piid")?,
            award_id_alt: None,
            modification_number: need("modification_number")?,
            uei: need("recipient_uei")?,
            recipient_name: need("recipient_name")?,
            agency: need("awarding_agency_name")?,
            action_date: need("action_date")?,
            obligation: need("federal_action_obligation")?,
            assistance_type: None,
        })
    } else if pos("assistance_transaction_unique_key").is_some() {
        Ok(Cols {
            kind: KIND_GRANT,
            transaction_key: need("assistance_transaction_unique_key")?,
            award_id: need("award_id_fain")?,
            award_id_alt: Some(need("award_id_uri")?),
            modification_number: need("modification_number")?,
            uei: need("recipient_uei")?,
            recipient_name: need("recipient_name")?,
            agency: need("awarding_agency_name")?,
            action_date: need("action_date")?,
            obligation: need("federal_action_obligation")?,
            assistance_type: Some(need("assistance_type_code")?),
        })
    } else {
        bail!("csv is neither a contract nor an assistance transaction file");
    }
}

fn keep_record(
    cols: &Cols,
    record: &StringRecord,
    ueis: &HashSet<String>,
    window_start: &str,
    window_end: &str,
) -> Option<AwardRow> {
    let uei = field(record, cols.uei).to_ascii_uppercase();
    if uei.len() != 12 || !uei.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    if !ueis.contains(&uei) {
        return None;
    }
    if cols.kind == KIND_GRANT {
        let code = grant_code(field(record, cols.assistance_type?));
        if !GRANT_CODES.contains(&code.as_str()) {
            return None;
        }
    }
    let action_date = action_date(field(record, cols.action_date))?;
    if action_date.as_str() < window_start || action_date.as_str() > window_end {
        return None;
    }
    let obligation = obligation(field(record, cols.obligation))?;
    let award_id = award_id(cols, record)?;
    let key = field(record, cols.transaction_key);
    if key.is_empty() {
        return None;
    }
    Some(AwardRow {
        transaction_key: key.to_string(),
        kind: cols.kind.to_string(),
        uei,
        recipient_name: field(record, cols.recipient_name).to_string(),
        award_id,
        modification_number: field(record, cols.modification_number).to_string(),
        agency: field(record, cols.agency).to_string(),
        action_date,
        obligation,
    })
}

fn field<'a>(record: &'a StringRecord, idx: usize) -> &'a str {
    record.get(idx).unwrap_or("").trim()
}

fn award_id(cols: &Cols, record: &StringRecord) -> Option<String> {
    let primary = field(record, cols.award_id);
    if !primary.is_empty() {
        return Some(primary.to_string());
    }
    let alt = cols.award_id_alt.and_then(|idx| {
        let value = field(record, idx);
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    });
    alt
}

fn action_date(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let date = &raw[..10];
    let chars: Vec<char> = date.chars().collect();
    if chars.len() != 10
        || chars[4] != '-'
        || chars[7] != '-'
        || !chars
            .iter()
            .enumerate()
            .all(|(i, c)| *c == '-' || c.is_ascii_digit() || i == 4 || i == 7)
    {
        return None;
    }
    if !date.chars().enumerate().all(|(i, c)| {
        if i == 4 || i == 7 {
            c == '-'
        } else {
            c.is_ascii_digit()
        }
    }) {
        return None;
    }
    Some(date.to_string())
}

fn obligation(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| *c != ',' && *c != '$' && !c.is_whitespace())
        .collect();
    if cleaned.is_empty() || cleaned == "-" || cleaned == "." {
        return None;
    }
    let body = cleaned.strip_prefix('-').unwrap_or(&cleaned);
    let mut dots = 0;
    if body.is_empty()
        || !body.chars().all(|c| {
            if c == '.' {
                dots += 1;
                dots == 1
            } else {
                c.is_ascii_digit()
            }
        })
    {
        return None;
    }
    Some(cleaned)
}

fn grant_code(raw: &str) -> String {
    let digits: String = raw.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return String::new();
    }
    format!("{digits:0>2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ueis() -> HashSet<String> {
        HashSet::from(["MJENSFHZJP11".to_string()])
    }

    fn scan(csv: &str) -> Vec<AwardRow> {
        let mut rows = Vec::new();
        scan_csv(csv.as_bytes(), &ueis(), "2025-10-01", "2026-10-08", |row| {
            rows.push(row);
            Ok(())
        })
        .unwrap();
        rows
    }

    #[test]
    fn contract_on_the_map_is_kept_and_parent_uei_is_ignored() {
        let csv = "\
contract_transaction_unique_key,award_id_piid,modification_number,recipient_uei,recipient_parent_uei,recipient_name,awarding_agency_name,action_date,federal_action_obligation
k1,PIID1,0,MJENSFHZJP11,NOTTHISTILE01,LOCKHEED MARTIN CORPORATION,Department of Defense,2026-03-01,\"1,250.50\"
k2,PIID2,0,NOTONTHEMAP1,MJENSFHZJP11,SOME OTHER NAME,Department of Defense,2026-03-01,10
k3,PIID3,0,MJENSFHZJP11,,LOCKHEED MARTIN CORPORATION,Department of Defense,2024-01-01,10
";
        let rows = scan(csv);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].transaction_key, "k1");
        assert_eq!(rows[0].kind, "contract");
        assert_eq!(rows[0].uei, "MJENSFHZJP11");
        assert_eq!(rows[0].obligation, "1250.50");
        assert_eq!(rows[0].award_id, "PIID1");
    }

    #[test]
    fn grants_stay_and_loans_and_payments_drop() {
        let csv = "\
assistance_transaction_unique_key,award_id_fain,award_id_uri,modification_number,assistance_type_code,recipient_uei,recipient_name,awarding_agency_name,action_date,federal_action_obligation
g1,FAIN1,,0,04,MJENSFHZJP11,LOCKHEED MARTIN CORPORATION,Department of Energy,2026-04-02,99
g2,FAIN2,,0,05,mjensfhzjp11,LOCKHEED MARTIN CORPORATION,Department of Energy,2026-04-02,-5
g3,FAIN3,,0,07,MJENSFHZJP11,LOCKHEED MARTIN CORPORATION,Department of Energy,2026-04-02,99
g4,FAIN4,,0,06,MJENSFHZJP11,LOCKHEED MARTIN CORPORATION,Department of Energy,2026-04-02,99
g5,,URI5,1,4,MJENSFHZJP11,LOCKHEED MARTIN CORPORATION,Department of Energy,2026-11-01,1
";
        let rows = scan(csv);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, "grant");
        assert_eq!(rows[0].award_id, "FAIN1");
        assert_eq!(rows[1].award_id, "FAIN2");
        assert_eq!(rows[1].obligation, "-5");
    }
}
