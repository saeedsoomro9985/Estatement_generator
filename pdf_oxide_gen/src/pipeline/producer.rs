//! Stage 1: async MongoDB cursor producer (only stage that touches MongoDB).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use futures::TryStreamExt;
use mongodb::{
    bson::{doc, RawDocumentBuf},
    options::{ClientOptions, FindOptions},
    Client,
};

use crate::mongo::MongoConfig;
use crate::perf::{self, PipelineTimings};

/// Owned raw BSON document moved through the pipeline (no full-dataset `Vec` buffer).
pub type RawBson = RawDocumentBuf;

/// Stream documents from MongoDB into a bounded channel (backpressure on `send`).
pub fn run_mongo_producer(
    config: &MongoConfig,
    limit: usize,
    statement_id: Option<&str>,
    mongo_batch_size: u32,
    raw_tx: Sender<RawBson>,
    produced: Arc<AtomicUsize>,
    timings: Arc<PipelineTimings>,
) -> Result<usize> {
    let rt = tokio::runtime::Runtime::new().context("Tokio runtime init failed (producer)")?;
    rt.block_on(stream_mongo_to_channel(
        config,
        limit,
        statement_id,
        mongo_batch_size,
        raw_tx,
        produced,
        timings,
    ))
}

async fn stream_mongo_to_channel(
    config: &MongoConfig,
    limit: usize,
    statement_id: Option<&str>,
    mongo_batch_size: u32,
    raw_tx: Sender<RawBson>,
    produced: Arc<AtomicUsize>,
    timings: Arc<PipelineTimings>,
) -> Result<usize> {
    eprintln!(
        "[pdf-oxide] [producer] connecting {} (db={}.{}) …",
        config.uri, config.database, config.collection
    );

    let connect_start = Instant::now();
    let mut options = ClientOptions::parse(&config.uri)
        .await
        .with_context(|| format!("Invalid MongoDB URI: {}", config.uri))?;
    options.connect_timeout = Some(Duration::from_secs(15));
    options.server_selection_timeout = Some(Duration::from_secs(15));

    let client = Client::with_options(options)
        .with_context(|| format!("MongoDB connect failed: {}", config.uri))?;

    let collection = client
        .database(&config.database)
        .collection::<RawBson>(&config.collection);

    eprintln!(
        "[pdf-oxide] [producer] connected in {:.3}s",
        perf::secs(connect_start.elapsed())
    );

    let filter = match statement_id {
        Some(id) if !id.is_empty() => doc! {
            "$or": [
                { "statementId": id },
                { "customer.customerId": id },
            ]
        },
        _ => doc! {},
    };

    let batch_size = mongo_batch_size.clamp(1, 10_000);
    let find_options = FindOptions::builder()
        .limit(limit as i64)
        .batch_size(batch_size)
        .build();

    eprintln!(
        "[pdf-oxide] [producer] streaming up to {} document(s) | batch_size={} | sort=none",
        limit, batch_size
    );

    let wall_start = Instant::now();
    let mut cursor = collection
        .find(filter)
        .with_options(find_options)
        .await
        .context("MongoDB find failed")?;

    let mut count = 0usize;
    while count < limit {
        let fetch_start = Instant::now();
        let next = cursor
            .try_next()
            .await
            .context("MongoDB cursor read failed")?;
        timings.add_mongo_fetch(fetch_start.elapsed());

        let Some(raw) = next else {
            break;
        };

        let send_start = Instant::now();
        raw_tx
            .send(raw)
            .map_err(|_| anyhow::anyhow!("Raw BSON channel closed (decode stage exited early)"))?;
        timings.add_channel_send_wait(send_start.elapsed());

        count += 1;
        let n = produced.fetch_add(1, Ordering::Relaxed) + 1;
        if n == 1 || n % 500 == 0 {
            eprintln!("[pdf-oxide] [producer] streamed {n} document(s)");
        }
    }

    eprintln!(
        "[pdf-oxide] [producer] finished | {} document(s) | wall={:.3}s | mongo_fetch={:.3}s | send_wait={:.3}s",
        count,
        perf::secs(wall_start.elapsed()),
        perf::secs(timings.mongo_fetch()),
        perf::secs(timings.channel_send_wait()),
    );

    Ok(count)
}
