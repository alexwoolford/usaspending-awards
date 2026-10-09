use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use chrono::Utc;
use clap::{Parser, Subcommand};
use usaspending_awards::allow::load_ueis;
use usaspending_awards::db::{last_run, live_count, lookup, open, open_work};
use usaspending_awards::ingest::{apply_rows, finish_log, ingest_archives, record_error};
use usaspending_awards::time::utc_date;

#[derive(Parser)]
#[command(
    name = "usaspending-awards",
    about = "Prime-award transactions for UEIs already on the exact CIK map.",
    version
)]
struct Cli {
    #[arg(
        long,
        global = true,
        default_value = "data/usaspending-awards.sqlite",
        env = "USASPENDING_AWARDS_SQLITE"
    )]
    db: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download FY Full files and keep transactions for mapped UEIs
    Ingest {
        #[arg(long, default_value = "/var/lib/uei-cik/uei-cik.sqlite")]
        uei_db: PathBuf,
        #[arg(long, default_value = "data/archive")]
        cache_dir: PathBuf,
    },
    /// Last ingest_runs row
    Status,
    /// Live transactions for one UEI
    Lookup { uei: String },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(|| std::io::LineWriter::new(std::io::stderr()))
        .init();

    match run() {
        Ok(code) => code,
        Err(err) => {
            tracing::error!("{err:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Ingest { uei_db, cache_dir } => {
            let started = Utc::now();
            let as_of = utc_date(started);
            let db = open_work(&cli.db)?;
            let ueis = match load_ueis(&uei_db) {
                Ok(ueis) => ueis,
                Err(err) => {
                    record_error(&db, &as_of, started, Utc::now())?;
                    return Err(err);
                }
            };
            tracing::info!(ueis = ueis.len(), "mapped UEIs");
            match ingest_archives(&db, &ueis, &cache_dir, &as_of) {
                Ok((files_read, rows_scanned)) => {
                    let finished = Utc::now();
                    let stats =
                        apply_rows(&db, &as_of, started, finished, files_read, rows_scanned)?;
                    finish_log(&stats, &as_of);
                    println!("live={}", live_count(&db)?);
                    Ok(ExitCode::SUCCESS)
                }
                Err(err) => {
                    record_error(&db, &as_of, started, Utc::now())?;
                    Err(err)
                }
            }
        }
        Command::Status => {
            let conn = open(&cli.db)?;
            match last_run(&conn)? {
                None => println!("no ingest_runs"),
                Some(row) => println!(
                    "as_of_date={} status={} files_read={} rows_scanned={} upserted={} retracted={} started_at={} finished_at={}",
                    row.as_of_date,
                    row.status,
                    row.files_read,
                    row.rows_scanned,
                    row.transactions_upserted,
                    row.transactions_retracted,
                    row.started_at,
                    row.finished_at
                ),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Lookup { uei } => {
            let conn = open(&cli.db)?;
            let rows = lookup(&conn, &uei)?;
            if rows.is_empty() {
                println!("no transactions");
            }
            for row in rows {
                println!(
                    "uei={} kind={} action_date={} obligation={} award_id={} agency={} recipient_name={} key={}",
                    row.uei,
                    row.kind,
                    row.action_date,
                    row.obligation,
                    row.award_id,
                    row.agency,
                    row.recipient_name,
                    row.transaction_key
                );
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
