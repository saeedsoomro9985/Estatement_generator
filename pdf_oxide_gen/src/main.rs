//! Avanza Bank Statement — Rust / pdf-oxide streaming batch generator

use anyhow::{Context, Result};
use clap::Parser;
use pdf_oxide_gen::{mongo::MongoConfig, pipeline};
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
    #[arg(default_value_t = 10)]
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
    #[arg(short = 'm', long, default_value = "batch")]
    mode: String,
    #[arg(short = 'c', long)]
    chunk_size: Option<usize>,
}

#[derive(Serialize)]
struct Output {
    generated: usize,
    duration: f64,
    tps: f64,
    workers: usize,
    mode: String,
    chunk_size: usize,
    /// Canonical output directory (same path writers use).
    output_dir: String,
    produced: usize,
    decoded: usize,
    rendered: usize,
    written: usize,
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

fn run() -> Result<()> {
    let args = Args::parse();
    eprintln!(
        "[pdf-oxide] start count={} output={} uri={} db={}.{}",
        args.count,
        args.output_dir.display(),
        args.mongo_uri,
        args.database,
        args.collection
    );

    let mongo_config = MongoConfig::new(&args.mongo_uri, &args.database, &args.collection);
    let limit = if args.statement_id.is_some() {
        1
    } else {
        args.count
    };

    let workers = args.workers.unwrap_or_else(default_workers);
    let chunk_size = args
        .chunk_size
        .unwrap_or_else(|| (limit / (workers * 2)).max(1));

    fs::create_dir_all(&args.output_dir)?;
    let output_dir = fs::canonicalize(&args.output_dir).unwrap_or(args.output_dir.clone());
    let output_dir_str = output_dir.display().to_string();
    eprintln!("[pdf-oxide] output directory: {output_dir_str}");

    let pipeline_cfg = pipeline::pipeline_config_from_workers(
        mongo_config,
        limit,
        args.statement_id.clone(),
        output_dir,
        workers,
        args.chunk_size,
    );

    eprintln!(
        "[pdf-oxide] {} PDFs | {} workers | chunk={} | mode={} | db={}.{}",
        limit,
        workers,
        chunk_size,
        args.mode,
        args.database,
        args.collection
    );

    let pipeline_start = Instant::now();
    let result = pipeline::run_pipeline(pipeline_cfg)?;
    let duration = pipeline_start.elapsed().as_secs_f64();
    let total = result.written;
    let tps = total as f64 / duration.max(f64::EPSILON);

    let out = Output {
        generated: total,
        duration: (duration * 1000.0).round() / 1000.0,
        tps: (tps * 100.0).round() / 100.0,
        workers,
        mode: args.mode.clone(),
        chunk_size,
        output_dir: output_dir_str,
        produced: result.produced,
        decoded: result.decoded,
        rendered: result.rendered,
        written: result.written,
        mongo_fetch_secs: result.mongo_fetch_secs,
        decode_secs: result.decode_secs,
    };

    println!("{}", serde_json::to_string(&out)?);
    eprintln!(
        "Done  {} PDFs | {:.2}s | {:.2} PDF/sec | fetch={:.3}s decode={:.3}s | produced={} decoded={} rendered={}",
        total,
        duration,
        tps,
        result.mongo_fetch_secs,
        result.decode_secs,
        result.produced,
        result.decoded,
        result.rendered,
    );

    Ok(())
}
