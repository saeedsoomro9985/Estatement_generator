//! Stage 2: fetch MongoDB statement by CIF, decode + map_statement.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};
use mongodb::bson::{doc, Document};
use mongodb::options::ClientOptions;
use mongodb::Client;
use tokio::runtime::Runtime;

use crate::customer::{map_statement, Customer};
use crate::mongo::MongoConfig;
use crate::perf::PipelineTimings;
use crate::sql::QueueItem;

/// Queue row + mapped customer ready for PDF rendering.
#[derive(Debug, Clone)]
pub struct EnrichedWork {
    pub queue: QueueItem,
    pub customer: Customer,
}

pub fn spawn_mongo_fetch_workers(
    worker_count: usize,
    mongo: MongoConfig,
    queue_rx: Receiver<QueueItem>,
    enriched_tx: Sender<EnrichedWork>,
    mongo_ok: Arc<AtomicUsize>,
    mongo_miss: Arc<AtomicUsize>,
    timings: Arc<PipelineTimings>,
    failure_tx: Sender<crate::pipeline::status_updater::StatusJob>,
) -> Vec<JoinHandle<Result<()>>> {
    (0..worker_count)
        .map(|id| {
            let qrx = queue_rx.clone();
            let etx = enriched_tx.clone();
            let ftx = failure_tx.clone();
            let cfg = mongo.clone();
            let ok = Arc::clone(&mongo_ok);
            let miss = Arc::clone(&mongo_miss);
            let timing = Arc::clone(&timings);
            thread::Builder::new()
                .name(format!("mongo-fetch-{id}"))
                .spawn(move || {
                    fetch_worker_loop(id, cfg, qrx, etx, ftx, ok, miss, timing)
                })
                .expect("spawn mongo fetch worker")
        })
        .collect()
}

fn fetch_worker_loop(
    worker_id: usize,
    mongo: MongoConfig,
    queue_rx: Receiver<QueueItem>,
    enriched_tx: Sender<EnrichedWork>,
    failure_tx: Sender<crate::pipeline::status_updater::StatusJob>,
    mongo_ok: Arc<AtomicUsize>,
    mongo_miss: Arc<AtomicUsize>,
    timings: Arc<PipelineTimings>,
) -> Result<()> {
    let rt = Runtime::new().context("tokio runtime (mongo fetch)")?;
    let client = rt.block_on(connect_mongo(&mongo))?;

    while let Ok(item) = queue_rx.recv() {
        let fetch_start = Instant::now();
        let result = rt.block_on(fetch_by_cif(&client, &mongo, &item.cif));
        timings.add_mongo_fetch(fetch_start.elapsed());

        match result {
            Ok(Some((customer, bson_d, map_d))) => {
                timings.add_bson_deserialize(bson_d);
                timings.add_map_statement(map_d);
                enriched_tx
                    .send(EnrichedWork {
                        queue: item,
                        customer,
                    })
                    .map_err(|_| anyhow::anyhow!("enriched channel closed"))?;
                let n = mongo_ok.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 || n % 500 == 0 {
                    eprintln!("[pdf-oxide] [mongo] fetched {n} statement(s)");
                }
            }
            Ok(None) => {
                mongo_miss.fetch_add(1, Ordering::Relaxed);
                let msg = format!("no Mongo document for CIF={}", item.cif);
                eprintln!("[pdf-oxide] [mongo] WARN: {msg}");
                let _ = failure_tx.send(crate::pipeline::status_updater::StatusJob::Failure {
                    queue_id: Some(item.id),
                    cif: item.cif.clone(),
                    stage: "mongo_fetch".into(),
                    message: msg,
                    retry: true,
                });
            }
            Err(e) => {
                mongo_miss.fetch_add(1, Ordering::Relaxed);
                let msg = format!("{e:#}");
                eprintln!(
                    "[pdf-oxide] [mongo] ERROR worker={worker_id} queue_id={} cif={}: {msg}",
                    item.id, item.cif
                );
                let _ = failure_tx.send(crate::pipeline::status_updater::StatusJob::Failure {
                    queue_id: Some(item.id),
                    cif: item.cif,
                    stage: "mongo_fetch".into(),
                    message: msg,
                    retry: true,
                });
            }
        }
    }
    Ok(())
}

async fn connect_mongo(mongo: &MongoConfig) -> Result<Client> {
    let mut options = ClientOptions::parse(&mongo.uri)
        .await
        .context("invalid mongo uri")?;
    Client::with_options(options).context("mongo connect failed")
}

async fn fetch_by_cif(
    client: &Client,
    mongo: &MongoConfig,
    cif: &str,
) -> Result<Option<(Customer, std::time::Duration, std::time::Duration)>> {
    let collection = client
        .database(&mongo.database)
        .collection::<Document>(&mongo.collection);

    let filter = doc! { "customer.cif": cif };
    let Some(doc) = collection.find_one(filter).await.context("mongo find_one")? else {
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
