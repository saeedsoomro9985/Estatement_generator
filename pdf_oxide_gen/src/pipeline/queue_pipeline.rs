//! SQL-driven producer–consumer pipeline (enterprise scale).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use anyhow::{Context, Result};
use crossbeam_channel::bounded;

use crate::mongo::MongoConfig;
use crate::perf::PipelineTimings;
use crate::pipeline::mongo_fetch;
use crate::pipeline::pdf_workers;
use crate::pipeline::queue_producer;
use crate::pipeline::status_updater::{self, StatusJob};
use crate::pipeline::writer;
use crate::sql::SqlConfig;

#[derive(Debug, Clone)]
pub struct QueuePipelineConfig {
    pub sql: SqlConfig,
    pub mongo: MongoConfig,
    pub output_dir: PathBuf,
    /// Max queue rows to process (0 = until SQL queue empty).
    pub max_records: usize,
    pub queue_batch_size: i32,
    pub poll_interval_ms: u64,
    pub channel_capacity: usize,
    pub mongo_fetch_workers: usize,
    pub pdf_workers: usize,
    pub writer_threads: usize,
    pub sql_batch_size: usize,
}

#[derive(Debug)]
pub struct QueuePipelineResult {
    pub claimed: usize,
    pub mongo_ok: usize,
    pub mongo_miss: usize,
    pub rendered: usize,
    pub written: usize,
    pub sql_completed: usize,
    pub sql_failed: usize,
    pub duration_secs: f64,
    pub mongo_fetch_secs: f64,
    pub decode_secs: f64,
}

pub fn run_queue_pipeline(config: QueuePipelineConfig) -> Result<QueuePipelineResult> {
    let start = Instant::now();
    let shutdown = Arc::new(AtomicBool::new(false));

    let claimed = Arc::new(AtomicUsize::new(0));
    let mongo_ok = Arc::new(AtomicUsize::new(0));
    let mongo_miss = Arc::new(AtomicUsize::new(0));
    let rendered = Arc::new(AtomicUsize::new(0));
    let written = Arc::new(AtomicUsize::new(0));
    let sql_completed = Arc::new(AtomicUsize::new(0));
    let sql_failed = Arc::new(AtomicUsize::new(0));
    let timings = PipelineTimings::new_shared();

    let cap = config.channel_capacity.max(64);

    eprintln!(
        "[pdf-oxide] queue-pipeline | machine={} | max_records={} | queue_batch={} | poll_ms={} | cap={} | mongo_w={} pdf_w={} writer_w={}",
        config.sql.machine_id,
        if config.max_records == 0 {
            "∞".into()
        } else {
            config.max_records.to_string()
        },
        config.queue_batch_size,
        config.poll_interval_ms,
        cap,
        config.mongo_fetch_workers,
        config.pdf_workers,
        config.writer_threads,
    );

    std::fs::create_dir_all(&config.output_dir).with_context(|| {
        format!(
            "create output dir {}",
            config.output_dir.display()
        )
    })?;

    let rt = tokio::runtime::Runtime::new().context("tokio runtime (pipeline bootstrap)")?;
    rt.block_on(crate::sql::test_connection(&config.sql))?;

    let (queue_tx, queue_rx) = bounded(cap);
    let (enriched_tx, enriched_rx) = bounded(cap);
    let (pdf_tx, pdf_rx) = bounded(cap);
    let (status_tx, status_rx) = bounded(cap);

    let status_handle = status_updater::spawn_status_updater(
        config.sql.clone(),
        config.sql_batch_size,
        status_rx,
        Arc::clone(&sql_completed),
        Arc::clone(&sql_failed),
    );

    let failure_tx = status_tx.clone();

    let writer_handles = writer::spawn_writer_workers(
        config.writer_threads,
        config.output_dir.clone(),
        pdf_rx,
        Arc::clone(&written),
        status_tx.clone(),
    );

    let pdf_handles = pdf_workers::spawn_pdf_workers(
        config.pdf_workers,
        enriched_rx,
        pdf_tx.clone(),
        Arc::clone(&rendered),
    );

    let mongo_handle = mongo_fetch::spawn_mongo_fetch_pool(
        config.mongo_fetch_workers,
        config.mongo.clone(),
        queue_rx,
        enriched_tx.clone(),
        Arc::clone(&mongo_ok),
        Arc::clone(&mongo_miss),
        Arc::clone(&timings),
        failure_tx,
    );

    let producer_handle = queue_producer::spawn_queue_producer(
        config.sql.clone(),
        config.queue_batch_size,
        config.poll_interval_ms,
        config.max_records,
        queue_tx,
        Arc::clone(&claimed),
        Arc::clone(&shutdown),
    );

    // ── Shutdown order: producer → mongo → pdf → writers → status ──
    producer_handle
        .join()
        .map_err(|_| anyhow::anyhow!("queue producer panicked"))??;

    mongo_handle
        .join()
        .map_err(|_| anyhow::anyhow!("mongo fetch pool panicked"))??;
    drop(enriched_tx);

    for h in pdf_handles {
        h.join()
            .map_err(|_| anyhow::anyhow!("pdf worker panicked"))??;
    }
    drop(pdf_tx);

    for h in writer_handles {
        h.join()
            .map_err(|_| anyhow::anyhow!("writer worker panicked"))??;
    }

    drop(status_tx);

    status_handle
        .join()
        .map_err(|_| anyhow::anyhow!("status updater panicked"))??;

    let claimed_n = claimed.load(Ordering::Relaxed);
    let written_n = written.load(Ordering::Relaxed);

    timings.log_fetch_vs_decode(claimed_n);

    eprintln!(
        "[pdf-oxide] queue-pipeline complete | claimed={claimed_n} mongo_ok={} mongo_miss={} rendered={} written={} sql_ok={} sql_fail={} | {:.3}s",
        mongo_ok.load(Ordering::Relaxed),
        mongo_miss.load(Ordering::Relaxed),
        rendered.load(Ordering::Relaxed),
        written_n,
        sql_completed.load(Ordering::Relaxed),
        sql_failed.load(Ordering::Relaxed),
        crate::perf::secs(start.elapsed()),
    );

    if claimed_n == 0 {
        anyhow::bail!("No rows claimed from Stmt_Request_Queue (GeneratedStatus=109)");
    }

    Ok(QueuePipelineResult {
        claimed: claimed_n,
        mongo_ok: mongo_ok.load(Ordering::Relaxed),
        mongo_miss: mongo_miss.load(Ordering::Relaxed),
        rendered: rendered.load(Ordering::Relaxed),
        written: written_n,
        sql_completed: sql_completed.load(Ordering::Relaxed),
        sql_failed: sql_failed.load(Ordering::Relaxed),
        duration_secs: start.elapsed().as_secs_f64(),
        mongo_fetch_secs: crate::perf::secs(timings.mongo_fetch()),
        decode_secs: crate::perf::secs(timings.decode_total()),
    })
}

pub fn queue_config_from_args(
    sql: SqlConfig,
    mongo: MongoConfig,
    output_dir: PathBuf,
    max_records: usize,
    workers: usize,
    queue_batch_size: i32,
    poll_interval_ms: u64,
    channel_capacity: Option<usize>,
    sql_batch_size: usize,
) -> QueuePipelineConfig {
    let cpus = workers.max(1);
    let qbatch = queue_batch_size.clamp(1, 10_000) as usize;
    let cap = channel_capacity
        .unwrap_or(qbatch.saturating_mul(4))
        .clamp(128, 20_000);
  // IO-bound Mongo + SQL claim; CPU-bound PDF rendering uses most cores.
    let mongo_workers = (cpus * 2).clamp(4, 32);
    let pdf_workers = cpus.clamp(2, 32);
    let writer_threads = (cpus / 2).clamp(2, 8);

    QueuePipelineConfig {
        sql,
        mongo,
        output_dir,
        max_records,
        queue_batch_size: queue_batch_size.clamp(1, 10_000),
        poll_interval_ms,
        channel_capacity: cap,
        mongo_fetch_workers: mongo_workers,
        pdf_workers,
        writer_threads,
        sql_batch_size: sql_batch_size.max(1),
    }
}
