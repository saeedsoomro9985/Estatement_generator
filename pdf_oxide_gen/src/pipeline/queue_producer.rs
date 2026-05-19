//! Stage 1: SQL queue producer — claims pending rows in transactional batches.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use tokio::runtime::Runtime;

use crate::sql::{self, SqlConfig};
use crate::sql::QueueItem;

pub fn spawn_queue_producer(
    sql: SqlConfig,
    queue_batch_size: i32,
    poll_interval_ms: u64,
    max_records: usize,
    queue_tx: Sender<QueueItem>,
    claimed_total: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
) -> JoinHandle<Result<()>> {
    thread::Builder::new()
        .name("sql-producer".into())
        .spawn(move || {
            producer_loop(
                sql,
                queue_batch_size,
                poll_interval_ms,
                max_records,
                queue_tx,
                claimed_total,
                shutdown,
            )
        })
        .expect("spawn sql producer")
}

fn producer_loop(
    sql: SqlConfig,
    batch_size: i32,
    poll_interval_ms: u64,
    max_records: usize,
    queue_tx: Sender<QueueItem>,
    claimed_total: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    let rt = Runtime::new().context("tokio runtime (sql producer)")?;
    let mut client = rt.block_on(sql::connect(&sql))?;

    eprintln!(
        "[pdf-oxide] [queue-producer] started | batch_size={} poll_ms={} max_records={}",
        batch_size,
        poll_interval_ms,
        if max_records == 0 { "∞".to_string() } else { max_records.to_string() }
    );

    let unlimited = max_records == 0;
    let mut empty_polls = 0u32;

    while !shutdown.load(Ordering::Relaxed) {
        if !unlimited && claimed_total.load(Ordering::Relaxed) >= max_records {
            break;
        }

        let remaining = if unlimited {
            batch_size
        } else {
            let left = max_records.saturating_sub(claimed_total.load(Ordering::Relaxed));
            (left as i32).min(batch_size).max(0)
        };

        if remaining == 0 {
            break;
        }

        let batch = rt.block_on(sql::claim_pending_batch(
            &mut client,
            remaining,
            &sql.machine_id,
        ))?;

        if batch.is_empty() {
            empty_polls += 1;
            if empty_polls == 1 || empty_polls % 10 == 0 {
                eprintln!(
                    "[pdf-oxide] [queue-producer] queue empty (poll #{empty_polls}), sleeping {poll_interval_ms}ms"
                );
            }
            thread::sleep(Duration::from_millis(poll_interval_ms));
            continue;
        }

        empty_polls = 0;
        let n = batch.len();
        for item in batch {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            queue_tx
                .send(item)
                .map_err(|_| anyhow::anyhow!("queue channel closed (downstream exited)"))?;
            claimed_total.fetch_add(1, Ordering::Relaxed);
        }

        let total = claimed_total.load(Ordering::Relaxed);
        eprintln!("[pdf-oxide] [queue-producer] claimed {n} row(s) | total_claimed={total}");
    }

    eprintln!(
        "[pdf-oxide] [queue-producer] finished | total_claimed={}",
        claimed_total.load(Ordering::Relaxed)
    );
    Ok(())
}
