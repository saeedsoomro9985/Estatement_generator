//! Avanza Bank Statement — Rust / pdf-oxide batch generator (SQL queue + Mongo + PDF).

use anyhow::{Context, Result};
use clap::Parser;
use pdf_oxide_gen::{
    mongo::MongoConfig,
    pipeline::{self, queue_config_from_args, run_queue_pipeline},
    sql::SqlConfig,
};
use serde::Serialize;
use std::{
    fs,
    path::PathBuf,
    thread::available_parallelism,
    time::Instant,
};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Max queue rows / PDFs (0 = process until SQL queue empty).
    #[arg(default_value_t = 0)]
    count: usize,
    #[arg(long, default_value = "mongodb://localhost:27017")]
    mongo_uri: String,
    #[arg(long, default_value = "EStatements")]
    database: String,
    #[arg(long, default_value = "Statements")]
    collection: String,
    #[arg(long)]
    statement_id: Option<String>,
    #[arg(short = 'o', long, default_value = "output")]
    output_dir: PathBuf,
    #[arg(short = 'w', long)]
    workers: Option<usize>,
    #[arg(short = 'm', long, default_value = "queue")]
    mode: String,
    #[arg(short = 'c', long)]
    chunk_size: Option<usize>,
    /// SQL Server ADO connection string.
    #[arg(long, env = "MSSQL_URL")]
    mssql_url: Option<String>,
    /// Machine identifier for queue claiming (required for queue mode).
    #[arg(long = "machine-id", env = "MACHINE_ID")]
    machine_id: Option<String>,
    /// Rows claimed per SQL batch (UPDLOCK/READPAST).
    #[arg(long = "queue-batch-size", env = "QUEUE_BATCH_SIZE", default_value_t = 100)]
    queue_batch_size: i32,
    /// Sleep when queue empty (ms).
    #[arg(long = "poll-interval-ms", env = "POLL_INTERVAL_MS", default_value_t = 500)]
    poll_interval_ms: u64,
    /// SQL status batch size (reserved / same as queue batch by default).
    #[arg(long = "sql-batch-size", env = "MSSQL_BATCH_SIZE", default_value_t = 50)]
    sql_batch_size: i32,
    /// Use legacy Mongo-direct pipeline (no SQL queue).
    #[arg(long = "legacy-mongo")]
    legacy_mongo: bool,
}

#[derive(Serialize)]
struct Output {
    generated: usize,
    duration: f64,
    tps: f64,
    workers: usize,
    mode: String,
    output_dir: String,
    machine_id: Option<String>,
    claimed: usize,
    mongo_ok: usize,
    mongo_miss: usize,
    rendered: usize,
    written: usize,
    sql_completed: usize,
    sql_failed: usize,
    mongo_fetch_secs: f64,
    decode_secs: f64,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("[pdf-oxide] ERROR: {:#}", e);
        std::process::exit(1);
    }
}

fn default_workers() -> usize {
    available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

fn build_mssql_url(args: &Args) -> String {
    if let Some(url) = &args.mssql_url {
        if !url.trim().is_empty() {
            return url.trim().to_string();
        }
    }
    let server = std::env::var("MSSQL_SERVER").unwrap_or_else(|_| "localhost\\SQLEXPRESS02".into());
    let port = std::env::var("MSSQL_PORT").ok();
    let server = if let Some(p) = port.filter(|s| !s.is_empty()) {
        format!("{server},{p}")
    } else {
        server
    };
    let database = std::env::var("MSSQL_DATABASE").unwrap_or_else(|_| "Statements".into());
    let user = std::env::var("MSSQL_USER").unwrap_or_else(|_| "sa".into());
    let password = std::env::var("MSSQL_PASSWORD").unwrap_or_else(|_| "Realme5i+123".into());
    format!(
        "Server={server};Database={database};User Id={user};Password={password};TrustServerCertificate=True;Encrypt=false"
    )
}

fn run() -> Result<()> {
    let args = Args::parse();
    let workers = args.workers.unwrap_or_else(default_workers);
    let mongo_config = MongoConfig::new(&args.mongo_uri, &args.database, &args.collection);

    fs::create_dir_all(&args.output_dir)?;
    let output_dir = fs::canonicalize(&args.output_dir).unwrap_or(args.output_dir.clone());
    let output_dir_str = output_dir.display().to_string();

    let use_legacy = args.legacy_mongo || args.mode == "mongo" || args.machine_id.is_none();

    let pipeline_start = Instant::now();

    if use_legacy {
        eprintln!("[pdf-oxide] mode=legacy-mongo-direct");
        let limit = if args.statement_id.is_some() {
            1
        } else {
            args.count.max(1)
        };
        let cfg = pipeline::pipeline_config_from_workers(
            mongo_config,
            limit,
            args.statement_id.clone(),
            output_dir,
            workers,
            args.chunk_size,
        );
        let result = pipeline::run_pipeline(cfg)?;
        let duration = pipeline_start.elapsed().as_secs_f64();
        let out = Output {
            generated: result.written,
            duration: (duration * 1000.0).round() / 1000.0,
            tps: result.written as f64 / duration.max(f64::EPSILON),
            workers,
            mode: "legacy-mongo".into(),
            output_dir: output_dir_str,
            machine_id: None,
            claimed: result.produced,
            mongo_ok: result.decoded,
            mongo_miss: 0,
            rendered: result.rendered,
            written: result.written,
            sql_completed: 0,
            sql_failed: 0,
            mongo_fetch_secs: result.mongo_fetch_secs,
            decode_secs: result.decode_secs,
        };
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

    let machine_id = args
        .machine_id
        .clone()
        .filter(|s| !s.trim().is_empty())
        .context("--machine-id is required for queue mode (or set MACHINE_ID)")?;

    let sql = SqlConfig::new(build_mssql_url(&args), machine_id.clone());
    let max_records = args.count;

    eprintln!(
        "[pdf-oxide] mode=sql-queue | machine={} | max_records={}",
        machine_id,
        if max_records == 0 {
            "until-empty".into()
        } else {
            max_records.to_string()
        }
    );

    let cfg = queue_config_from_args(
        sql,
        mongo_config,
        output_dir,
        max_records,
        workers,
        args.queue_batch_size,
        args.poll_interval_ms,
        args.chunk_size.map(|c| c.max(args.sql_batch_size as usize)),
    );

    let result = run_queue_pipeline(cfg)?;
    let duration = pipeline_start.elapsed().as_secs_f64();

    let out = Output {
        generated: result.written,
        duration: (duration * 1000.0).round() / 1000.0,
        tps: result.written as f64 / duration.max(f64::EPSILON),
        workers,
        mode: args.mode,
        output_dir: output_dir_str,
        machine_id: Some(machine_id),
        claimed: result.claimed,
        mongo_ok: result.mongo_ok,
        mongo_miss: result.mongo_miss,
        rendered: result.rendered,
        written: result.written,
        sql_completed: result.sql_completed,
        sql_failed: result.sql_failed,
        mongo_fetch_secs: result.mongo_fetch_secs,
        decode_secs: result.decode_secs,
    };

    println!("{}", serde_json::to_string(&out)?);
    eprintln!(
        "Done  {} PDFs | {:.2}s | claimed={} sql_ok={} sql_fail={}",
        result.written,
        duration,
        result.claimed,
        result.sql_completed,
        result.sql_failed
    );

    Ok(())
}
