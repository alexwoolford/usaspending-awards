CREATE TABLE IF NOT EXISTS transactions (
    transaction_key TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('contract', 'grant')),
    uei TEXT NOT NULL,
    recipient_name TEXT NOT NULL,
    award_id TEXT NOT NULL,
    modification_number TEXT NOT NULL,
    agency TEXT NOT NULL,
    action_date TEXT NOT NULL,
    obligation TEXT NOT NULL,
    as_of_date TEXT NOT NULL,
    deleted_at INTEGER
) STRICT;

CREATE TABLE IF NOT EXISTS ingest_runs (
    as_of_date TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ok', 'error')),
    files_read INTEGER NOT NULL,
    rows_scanned INTEGER NOT NULL,
    transactions_upserted INTEGER NOT NULL,
    transactions_retracted INTEGER NOT NULL
) STRICT;

-- Not captured. Filled during a run and promoted only after every file succeeds.
CREATE TABLE IF NOT EXISTS transaction_stage (
    transaction_key TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    uei TEXT NOT NULL,
    recipient_name TEXT NOT NULL,
    award_id TEXT NOT NULL,
    modification_number TEXT NOT NULL,
    agency TEXT NOT NULL,
    action_date TEXT NOT NULL,
    obligation TEXT NOT NULL
) STRICT;
