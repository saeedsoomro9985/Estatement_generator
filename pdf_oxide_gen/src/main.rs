/*!
 * Avanza Bank Statement — Rust / pdf-oxide generator
 * ===================================================
 * Loads statements from MongoDB (EStatements.Statements), maps them to the
 * PDF model, and generates PDFs with the same layout as the ReportLab version.
 *
 * Build:   cargo build --release
 * Binary:  ./target/release/pdf_oxide_gen
 *
 * Usage:
 *   pdf_oxide_gen <count> \
 *       --output-dir output \
 *       --mongo-uri mongodb://localhost:27017 \
 *       --database EStatements \
 *       --collection Statements
 *
 * Stdout:  JSON  { generated, duration, tps, workers, chunk_size }
 */

use anyhow::{Context, Result};
use clap::Parser;
use pdf_oxide_gen::{map_statement, mongo, perf, render_pdf, MongoConfig};
use rayon::prelude::*;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use tokio::runtime::Runtime;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Maximum number of statements to fetch from MongoDB
    #[arg(default_value_t = 10)]
    count: usize,

    /// MongoDB connection URI
    #[arg(long, default_value = "mongodb://localhost:27017")]
    mongo_uri: String,

    /// Database name
    #[arg(long, default_value = "EStatements")]
    database: String,

    /// Collection name
    #[arg(long, default_value = "Statements")]
    collection: String,

    /// Fetch a single statement by statementId or customer.customerId
    #[arg(long)]
    statement_id: Option<String>,

    /// Output directory
    #[arg(short = 'o', long, default_value = "output")]
    output_dir: PathBuf,

    /// Number of parallel workers (defaults to logical CPUs)
    #[arg(short = 'w', long)]
    workers: Option<usize>,

    /// Dispatch mode: batch | single
    #[arg(short = 'm', long, default_value = "batch")]
    mode: String,

    /// Chunk size (batch mode; defaults to count / workers*2)
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
}

fn main() {
    if let Err(e) = run() {
        eprintln!("[pdf-oxide] ERROR: {:#}", e);
        std::process::exit(1);
    }
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

    let fetch = Runtime::new()
        .context("Tokio runtime init failed")?
        .block_on(mongo::fetch_statements(
            &mongo_config,
            limit,
            args.statement_id.as_deref(),
        ))?;

    if fetch.documents.is_empty() {
        anyhow::bail!(
            "No statements found in {}.{} (uri: {})",
            args.database,
            args.collection,
            args.mongo_uri
        );
    }

    let count = if args.statement_id.is_some() {
        fetch.documents.len()
    } else {
        args.count.min(fetch.documents.len())
    };

    let workers = args.workers.unwrap_or_else(|| rayon::current_num_threads());
    let output_dir = &args.output_dir;
    fs::create_dir_all(output_dir)?;

    let map_start = Instant::now();
    let customers: Vec<_> = fetch.documents[..count]
        .iter()
        .map(map_statement)
        .collect();
    let map_elapsed = map_start.elapsed();

    eprintln!(
        "[pdf-oxide] map_statement done: {} document(s) in {:.3}s ({:.2}ms/doc)",
        count,
        perf::secs(map_elapsed),
        map_elapsed.as_secs_f64() * 1000.0 / count.max(1) as f64,
    );

    let stage_timings = perf::StageTimings::from_mongo(&fetch.metrics).with_map_statement(map_elapsed);
    perf::StageTimings::log_mongo_detail(&fetch.metrics, count);
    stage_timings.log_summary(count);

    let chunk_size = args
        .chunk_size
        .unwrap_or_else(|| (count / (workers * 2)).max(1));

    eprintln!(
        "[pdf-oxide] {} PDFs from MongoDB | {} workers | chunk={} | mode={} | db={}.{}",
        count, workers, chunk_size, args.mode, args.database, args.collection
    );

    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .ok();

    let generated = AtomicUsize::new(0);
    let start = Instant::now();

    customers
        .par_chunks(chunk_size)
        .try_for_each(|chunk| -> Result<()> {
            for customer in chunk {
                let pdf_bytes = render_pdf(customer).with_context(|| {
                    format!("PDF render failed for customer id={}", customer.id)
                })?;
                let path = Path::new(output_dir).join(format!("OXIDE-{}.pdf", customer.id));
                fs::write(&path, &pdf_bytes)
                    .with_context(|| format!("Write failed: {}", path.display()))?;
                let n = generated.fetch_add(1, Ordering::Relaxed) + 1;
                if n % 25 == 0 || n == count {
                    eprintln!("[pdf-oxide] progress {}/{}", n, count);
                }
            }
            Ok(())
        })?;

    let total = generated.load(Ordering::Relaxed);
    let duration = start.elapsed().as_secs_f64();
    let tps = total as f64 / duration.max(f64::EPSILON);

    let out = Output {
        generated: total,
        duration: (duration * 1000.0).round() / 1000.0,
        tps: (tps * 100.0).round() / 100.0,
        workers,
        mode: args.mode,
        chunk_size,
    };

    println!("{}", serde_json::to_string(&out)?);
    eprintln!("Done  {} PDFs | {:.2}s | {:.2} PDF/sec", total, duration, tps);

    Ok(())
}
