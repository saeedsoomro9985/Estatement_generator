//! Streaming producer–consumer pipeline for large-scale PDF batch generation.

mod decode;
mod pdf_workers;
mod producer;
mod writer;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use anyhow::{Context, Result};
use crossbeam_channel::{bounded, unbounded};

use crate::mongo::MongoConfig;
use crate::perf;

pub use decode::DecodedCustomer;
pub use pdf_workers::PdfArtifact;
pub use producer::RawBson;

/// Tunable pipeline layout (bounded channels between every stage).
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub mongo: MongoConfig,
    pub limit: usize,
    pub statement_id: Option<String>,
    pub output_dir: PathBuf,
    pub decode_workers: usize,
    pub pdf_workers: usize,
    pub writer_threads: usize,
    /// Backpressure on Mongo ingress only (downstream uses unbounded queues).
    pub raw_channel_capacity: usize,
    /// MongoDB cursor `batch_size` (documents per server round-trip).
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

/// Run the full streaming pipeline. Only the Mongo producer thread talks to MongoDB.
pub fn run_pipeline(config: PipelineConfig) -> Result<PipelineResult> {
    let start = Instant::now();
    let progress = PipelineProgress::new();

    let timings = perf::PipelineTimings::new_shared();

    eprintln!(
        "[pdf-oxide] pipeline | decode={} pdf={} writer={} | raw_channel={} | mongo_batch={}",
        config.decode_workers,
        config.pdf_workers,
        config.writer_threads,
        config.raw_channel_capacity,
        config.mongo_batch_size,
    );

    std::fs::create_dir_all(&config.output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            config.output_dir.display()
        )
    })?;

    // Bounded only at Mongo ingress; unbounded downstream avoids decode/pdf deadlocks.
    let (raw_tx, raw_rx) = bounded(config.raw_channel_capacity);
    let (customer_tx, customer_rx) = unbounded();
    let (pdf_tx, pdf_rx) = unbounded();

    let decode_handles = decode::spawn_decode_workers(
        config.decode_workers,
        raw_rx,
        customer_tx.clone(),
        Arc::clone(&progress.decoded),
        Arc::clone(&timings),
    );

    let pdf_handles = pdf_workers::spawn_pdf_workers(
        config.pdf_workers,
        customer_rx,
        pdf_tx.clone(),
        Arc::clone(&progress.rendered),
    );

    let writer_handles = writer::spawn_writer_workers(
        config.writer_threads,
        config.output_dir.clone(),
        pdf_rx,
        Arc::clone(&progress.written),
    );

    let producer_cfg = config.clone();
    let producer_progress = Arc::clone(&progress.produced);
    let producer_timings = Arc::clone(&timings);
    let producer_handle: JoinHandle<Result<usize>> = std::thread::spawn(move || {
        producer::run_mongo_producer(
            &producer_cfg.mongo,
            producer_cfg.limit,
            producer_cfg.statement_id.as_deref(),
            producer_cfg.mongo_batch_size,
            raw_tx,
            producer_progress,
            producer_timings,
        )
    });

    let produced = producer_handle
        .join()
        .map_err(|_| anyhow::anyhow!("Mongo producer thread panicked"))??;

    for h in decode_handles {
        h.join()
            .map_err(|_| anyhow::anyhow!("Decode worker panicked"))??;
    }
    drop(customer_tx);

    for h in pdf_handles {
        h.join()
            .map_err(|_| anyhow::anyhow!("PDF worker panicked"))??;
    }
    drop(pdf_tx);

    for h in writer_handles {
        h.join()
            .map_err(|_| anyhow::anyhow!("Writer worker panicked"))??;
    }

    let decoded = progress.decoded.load(Ordering::Relaxed);
    let rendered = progress.rendered.load(Ordering::Relaxed);
    let written = progress.written.load(Ordering::Relaxed);

    timings.log_fetch_vs_decode(produced);

    eprintln!(
        "[pdf-oxide] pipeline complete | produced={} decoded={} rendered={} written={} | {:.3}s",
        produced,
        decoded,
        rendered,
        written,
        perf::secs(start.elapsed()),
    );

    if produced == 0 {
        anyhow::bail!(
            "No statements streamed from {}.{} (uri: {})",
            config.mongo.database,
            config.mongo.collection,
            config.mongo.uri
        );
    }
    if written == 0 {
        anyhow::bail!(
            "No PDF files were written to {}",
            config.output_dir.display()
        );
    }

    Ok(PipelineResult {
        produced,
        decoded,
        rendered,
        written,
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
    let pdf_workers = cpus.saturating_sub(1).max(1);
    let decode_workers = 2.min(limit.max(1));
    let writer_threads = 2;
    let raw_cap = chunk_size_hint.unwrap_or(512).clamp(64, 5000);
    let mongo_batch_size = chunk_size_hint.unwrap_or(100).clamp(16, 2000) as u32;

    PipelineConfig {
        mongo,
        limit,
        statement_id,
        output_dir,
        decode_workers,
        pdf_workers,
        writer_threads,
        raw_channel_capacity: raw_cap,
        mongo_batch_size,
    }
}
