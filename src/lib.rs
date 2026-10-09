//! Prime-award transactions whose recipient UEI is already on the exact CIK map.
//! This tile does not decide the UEI-to-CIK link.

pub mod allow;
pub mod archive;
pub mod db;
pub mod ingest;
pub mod parse;
pub mod time;

pub use allow::load_ueis;
pub use db::{open, open_work, WorkDb, DB_NAME};
pub use ingest::{apply_rows, record_error, IngestStats};
pub use parse::{scan_csv, AwardRow};
