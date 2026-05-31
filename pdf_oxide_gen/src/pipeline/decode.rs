//! Stage 2: parallel BSON decode + `map_statement` (no MongoDB access).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};
use mongodb::bson;

use crate::customer::{map_statement, Statement};
use crate::perf::PipelineTimings;
use crate::pipeline::producer::RawBson;
use crate::statement::StatementDocument;

/// Mapped customer ready for PDF rendering.
pub type DecodedCustomer = Statement;

pub fn spawn_decode_workers(
    worker_count: usize,
    raw_rx: Receiver<RawBson>,
    customer_tx: Sender<DecodedCustomer>,
    decoded: Arc<AtomicUsize>,
    timings: Arc<PipelineTimings>,
) -> Vec<JoinHandle<Result<()>>> {
    (0..worker_count)
        .map(|id| {
            let rx = raw_rx.clone();
            let tx = customer_tx.clone();
            let progress = Arc::clone(&decoded);
            let timing = Arc::clone(&timings);
            thread::Builder::new()
                .name(format!("decode-{id}"))
                .spawn(move || decode_worker_loop(id, rx, tx, progress, timing))
                .expect("spawn decode worker")
        })
        .collect()
}

fn decode_worker_loop(
    worker_id: usize,
    raw_rx: Receiver<RawBson>,
    customer_tx: Sender<DecodedCustomer>,
    decoded: Arc<AtomicUsize>,
    timings: Arc<PipelineTimings>,
) -> Result<()> {
    while let Ok(raw) = raw_rx.recv() {
        let bson_start = Instant::now();
        let statement: StatementDocument = bson::from_slice(raw.as_bytes())
            .with_context(|| format!("BSON deserialize failed (decode worker {worker_id})"))?;
        timings.add_bson_deserialize(bson_start.elapsed());

        let map_start = Instant::now();
        let customer = map_statement(&statement);
        timings.add_map_statement(map_start.elapsed());

        customer_tx.send(customer).map_err(|_| {
            anyhow::anyhow!("Customer channel closed (PDF stage exited early)")
        })?;

        let n = decoded.fetch_add(1, Ordering::Relaxed) + 1;
        if n % 500 == 0 {
            eprintln!("[pdf-oxide] [decode] mapped {n} customer(s)");
        }
    }
    Ok(())
}
