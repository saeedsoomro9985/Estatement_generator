//! Streaming pipelines: SQL queue (primary) and legacy Mongo-direct.

mod decode;
mod legacy_pdf;
mod mongo_fetch;
mod pdf_workers;
mod producer;
mod queue_pipeline;
mod queue_producer;
mod status_updater;
mod writer;

pub use mongo_fetch::EnrichedWork;
pub use pdf_workers::PdfArtifact;
pub use producer::RawBson;
pub use queue_pipeline::{
    queue_config_from_args, run_queue_pipeline, QueuePipelineConfig, QueuePipelineResult,
};

pub use self::queue_pipeline::run_queue_pipeline as run_sql_queue_pipeline;

// Legacy Mongo-direct pipeline (kept for local testing without SQL queue).
pub use decode::DecodedCustomer;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use anyhow::{Context, Result};
use crossbeam_channel::{bounded, unbounded};

use crate::mongo::MongoConfig;
use crate::perf;

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub mongo: MongoConfig,
    pub limit: usize,
    pub statement_id: Option<String>,
    pub output_dir: PathBuf,
    pub decode_workers: usize,
    pub pdf_workers: usize,
    pub writer_threads: usize,
    pub raw_channel_capacity: usize,
    pub mongo_batch_size: u32,
}

#[derive(Debug)]
pub struct PipelineResult {
    pub produced: usize,
    pub decoded: usize,
    pub rendered: usize,
    pub written: usize,
    pub duration_secs: f64,
    pub mongo_fetch_secs: f64,
    pub decode_secs: f64,
}

struct PipelineProgress {
    produced: Arc<AtomicUsize>,
    decoded: Arc<AtomicUsize>,
    rendered: Arc<AtomicUsize>,
    written: Arc<AtomicUsize>,
}

impl PipelineProgress {
    fn new() -> Self {
        Self {
            produced: Arc::new(AtomicUsize::new(0)),
            decoded: Arc::new(AtomicUsize::new(0)),
            rendered: Arc::new(AtomicUsize::new(0)),
            written: Arc::new(AtomicUsize::new(0)),
        }
    }
}

/// Legacy: MongoDB cursor → PDF (no SQL queue).
pub fn run_pipeline(config: PipelineConfig) -> Result<PipelineResult> {
    let start = Instant::now();
    let progress = PipelineProgress::new();
    let timings = perf::PipelineTimings::new_shared();

    eprintln!(
        "[pdf-oxide] legacy mongo-pipeline | decode={} pdf={} writer={}",
        config.decode_workers, config.pdf_workers, config.writer_threads
    );

    std::fs::create_dir_all(&config.output_dir)?;

    let (raw_tx, raw_rx) = bounded(config.raw_channel_capacity);
    let (customer_tx, customer_rx) = bounded(config.raw_channel_capacity);
    let (pdf_tx, pdf_rx) = bounded(config.raw_channel_capacity);
    let (status_tx, status_rx) = bounded(1024);
    std::thread::spawn(move || {
        while status_rx.recv().is_ok() {}
    });

    let decode_handles = decode::spawn_decode_workers(
        config.decode_workers,
        raw_rx,
        customer_tx.clone(),
        Arc::clone(&progress.decoded),
        Arc::clone(&timings),
    );

    let writer_handles = writer::spawn_writer_workers(
        config.writer_threads,
        config.output_dir.clone(),
        pdf_rx,
        Arc::clone(&progress.written),
        status_tx,
    );

    let pdf_handles = legacy_pdf::spawn_legacy_pdf_workers(
        config.pdf_workers,
        customer_rx,
        pdf_tx.clone(),
        Arc::clone(&progress.rendered),
    );

    let producer_timings = Arc::clone(&timings);
    let producer_handle: JoinHandle<Result<usize>> = std::thread::spawn(move || {
        producer::run_mongo_producer(
            &config.mongo,
            config.limit,
            config.statement_id.as_deref(),
            config.mongo_batch_size,
            raw_tx,
            Arc::clone(&progress.produced),
            producer_timings,
        )
    });

    let produced = producer_handle.join().map_err(|_| anyhow::anyhow!("producer panicked"))??;

    for h in decode_handles {
        h.join().map_err(|_| anyhow::anyhow!("decode panicked"))??;
    }
    drop(customer_tx);

    for h in pdf_handles {
        h.join().map_err(|_| anyhow::anyhow!("pdf panicked"))??;
    }
    drop(pdf_tx);
    for h in writer_handles {
        h.join().map_err(|_| anyhow::anyhow!("writer panicked"))??;
    }

    let produced_n = produced;
    timings.log_fetch_vs_decode(produced_n);

    Ok(PipelineResult {
        produced: produced_n,
        decoded: progress.decoded.load(Ordering::Relaxed),
        rendered: progress.rendered.load(Ordering::Relaxed),
        written: progress.written.load(Ordering::Relaxed),
        duration_secs: start.elapsed().as_secs_f64(),
        mongo_fetch_secs: perf::secs(timings.mongo_fetch()),
        decode_secs: perf::secs(timings.decode_total()),
    })
}

pub fn pipeline_config_from_workers(
    mongo: MongoConfig,
    limit: usize,
    statement_id: Option<String>,
    output_dir: PathBuf,
    workers: usize,
    chunk_size_hint: Option<usize>,
) -> PipelineConfig {
    let cpus = workers.max(1);
    let raw_cap = chunk_size_hint.unwrap_or(512).clamp(64, 5000);
    PipelineConfig {
        mongo,
        limit,
        statement_id,
        output_dir,
        decode_workers: 2.min(limit.max(1)),
        pdf_workers: cpus.saturating_sub(1).max(1),
        writer_threads: 2,
        raw_channel_capacity: raw_cap,
        mongo_batch_size: chunk_size_hint.unwrap_or(100).clamp(16, 2000) as u32,
    }
}
