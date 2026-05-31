//! Stage 2: concurrent MongoDB fetch by CIF (shared connection pool + tokio worker pool).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};
use mongodb::bson::{doc, Document};
use mongodb::options::{ClientOptions, FindOneOptions};
use mongodb::Client;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::customer::{map_statement, Statement};
use crate::mongo::MongoConfig;
use crate::perf::PipelineTimings;
use crate::pipeline::status_updater::StatusJob;
use crate::sql::QueueItem;

/// Queue row + mapped customer ready for PDF rendering.
#[derive(Debug, Clone)]
pub struct EnrichedWork {
    pub queue: QueueItem,
    pub customer: Statement,
}

/// One async worker pool (multi-thread tokio) instead of N separate runtimes.
pub fn spawn_mongo_fetch_pool(
    concurrency: usize,
    mongo: MongoConfig,
    queue_rx: Receiver<QueueItem>,
    enriched_tx: Sender<EnrichedWork>,
    mongo_ok: Arc<AtomicUsize>,
    mongo_miss: Arc<AtomicUsize>,
    timings: Arc<PipelineTimings>,
    failure_tx: Sender<StatusJob>,
) -> JoinHandle<Result<()>> {
    thread::Builder::new()
        .name("mongo-pool".into())
        .spawn(move || mongo_pool_loop(concurrency, mongo, queue_rx, enriched_tx, mongo_ok, mongo_miss, timings, failure_tx))
        .expect("spawn mongo pool")
}

fn mongo_pool_loop(
    concurrency: usize,
    mongo: MongoConfig,
    queue_rx: Receiver<QueueItem>,
    enriched_tx: Sender<EnrichedWork>,
    mongo_ok: Arc<AtomicUsize>,
    mongo_miss: Arc<AtomicUsize>,
    timings: Arc<PipelineTimings>,
    failure_tx: Sender<StatusJob>,
) -> Result<()> {
    let workers = concurrency.max(2).min(64);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .thread_name("mongo-io")
        .enable_all()
        .build()
        .context("tokio multi-thread runtime (mongo pool)")?;

    eprintln!("[pdf-oxide] [mongo] pool started | concurrency={workers}");

    let mongo_cfg = mongo.clone();
    rt.block_on(async move {
        let client = connect_mongo(&mongo_cfg).await?;
        let sem = Arc::new(Semaphore::new(workers));
        let mut in_flight: JoinSet<Result<()>> = JoinSet::new();
        const MAX_IN_FLIGHT: usize = 256;

        while let Ok(item) = queue_rx.recv() {
            while in_flight.len() >= MAX_IN_FLIGHT {
                if let Some(res) = in_flight.join_next().await {
                    res.context("mongo task join")??;
                }
            }

            let permit = sem
                .clone()
                .acquire_owned()
                .await
                .context("mongo semaphore")?;
            let client = client.clone();
            let mongo_cfg = mongo_cfg.clone();
            let etx = enriched_tx.clone();
            let ftx = failure_tx.clone();
            let ok = Arc::clone(&mongo_ok);
            let miss = Arc::clone(&mongo_miss);
            let timing = Arc::clone(&timings);

            in_flight.spawn(async move {
                let _permit = permit;
                let fetch_start = Instant::now();
                let result = fetch_by_cif(&client, &mongo_cfg, &item.cif).await;
                timing.add_mongo_fetch(fetch_start.elapsed());

                match result {
                    Ok(Some((customer, bson_d, map_d))) => {
                        timing.add_bson_deserialize(bson_d);
                        timing.add_map_statement(map_d);
                        etx.send(EnrichedWork { queue: item, customer })
                            .map_err(|_| anyhow::anyhow!("enriched channel closed"))?;
                        let n = ok.fetch_add(1, Ordering::Relaxed) + 1;
                        if n == 1 || n % 500 == 0 {
                            eprintln!("[pdf-oxide] [mongo] fetched {n} statement(s)");
                        }
                    }
                    Ok(None) => {
                        miss.fetch_add(1, Ordering::Relaxed);
                        let msg = format!("no Mongo document for CIF={}", item.cif);
                        let _ = ftx.send(StatusJob::Failure {
                            queue_id: Some(item.id),
                            cif: item.cif,
                            stage: "mongo_fetch".into(),
                            message: msg,
                            retry: true,
                        });
                    }
                    Err(e) => {
                        miss.fetch_add(1, Ordering::Relaxed);
                        let _ = ftx.send(StatusJob::Failure {
                            queue_id: Some(item.id),
                            cif: item.cif,
                            stage: "mongo_fetch".into(),
                            message: format!("{e:#}"),
                            retry: true,
                        });
                    }
                }
                Ok::<(), anyhow::Error>(())
            });
        }

        while let Some(res) = in_flight.join_next().await {
            res.context("mongo task join")??;
        }
        Ok::<(), anyhow::Error>(())
    })?;

    eprintln!("[pdf-oxide] [mongo] pool finished");
    Ok(())
}

async fn connect_mongo(mongo: &MongoConfig) -> Result<Client> {
    let mut options = ClientOptions::parse(&mongo.uri)
        .await
        .context("invalid mongo uri")?;
    options.min_pool_size = Some(2);
    options.max_pool_size = Some(50);
    options.max_idle_time = Some(std::time::Duration::from_secs(120));
    Client::with_options(options).context("mongo connect failed")
}

async fn fetch_by_cif(
    client: &Client,
    mongo: &MongoConfig,
    cif: &str,
) -> Result<Option<(Statement, std::time::Duration, std::time::Duration)>> {
    let collection = client
        .database(&mongo.database)
        .collection::<Document>(&mongo.collection);

    let filter = doc! { "customer.cif": cif };
    let find_options = FindOneOptions::builder()
        .build();

    let Some(doc) = collection
        .find_one(filter)
        .with_options(find_options)
        .await
        .context("mongo find_one")?
    else {
        return Ok(None);
    };

    let bson_start = Instant::now();
    let statement = mongodb::bson::from_document::<crate::statement::StatementDocument>(doc)
        .context("bson deserialize")?;
    let bson_d = bson_start.elapsed();
    let map_start = Instant::now();
    let customer = map_statement(&statement);
    let map_d = map_start.elapsed();
    Ok(Some((customer, bson_d, map_d)))
}
