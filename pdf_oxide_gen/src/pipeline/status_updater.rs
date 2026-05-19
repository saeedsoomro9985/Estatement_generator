//! Stage 5: batched SQL status updates (single connection, fewer round-trips).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use anyhow::{Context, Result};
use crossbeam_channel::Receiver;
use tokio::runtime::Runtime;

use crate::perf;
use crate::sql::{self, SqlConfig};

#[derive(Debug, Clone)]
pub enum StatusJob {
    Success {
        queue_id: i64,
        cif: String,
        file_path: String,
        file_name: String,
    },
    Failure {
        queue_id: Option<i64>,
        cif: String,
        stage: String,
        message: String,
        retry: bool,
    },
}

pub fn spawn_status_updater(
    sql: SqlConfig,
    sql_batch_size: usize,
    status_rx: Receiver<StatusJob>,
    completed: Arc<AtomicUsize>,
    failed: Arc<AtomicUsize>,
) -> JoinHandle<Result<()>> {
    thread::Builder::new()
        .name("sql-status".into())
        .spawn(move || status_loop(sql, sql_batch_size, status_rx, completed, failed))
        .expect("spawn status updater")
}

fn status_loop(
    sql: SqlConfig,
    sql_batch_size: usize,
    status_rx: Receiver<StatusJob>,
    completed: Arc<AtomicUsize>,
    failed: Arc<AtomicUsize>,
) -> Result<()> {
    let rt = Runtime::new().context("tokio runtime (status updater)")?;
    let mut client = rt.block_on(sql::connect(&sql))?;

    let batch_cap = sql_batch_size.max(1).min(sql::SQL_UPDATE_CHUNK);
    let mut success_ids: Vec<i64> = Vec::with_capacity(batch_cap);
    let mut retry_ids: Vec<i64> = Vec::new();

    eprintln!("[pdf-oxide] [sql-status] started | flush_batch={batch_cap}");

    let flush_success =
        |client: &mut sql::SqlClient, buf: &mut Vec<i64>, completed: &AtomicUsize| -> Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        let start = Instant::now();
        let n = rt.block_on(sql::mark_generated_batch(client, buf))?;
        completed.fetch_add(n, Ordering::Relaxed);
        eprintln!(
            "[pdf-oxide] [sql-status] batch generated {n} row(s) in {:.3}s",
            perf::secs(start.elapsed())
        );
        buf.clear();
        Ok(())
    };

    let flush_retry = |client: &mut sql::SqlClient, buf: &mut Vec<i64>| -> Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        let start = Instant::now();
        let n = rt.block_on(sql::mark_retry_pending_batch(client, buf))?;
        eprintln!(
            "[pdf-oxide] [sql-status] batch retry {n} row(s) in {:.3}s",
            perf::secs(start.elapsed())
        );
        buf.clear();
        Ok(())
    };

    while let Ok(job) = status_rx.recv() {
        match job {
            StatusJob::Success {
                queue_id,
                cif,
                file_path: _,
                file_name,
            } => {
                success_ids.push(queue_id);
                if success_ids.len() >= batch_cap {
                    if let Err(e) = flush_success(&mut client, &mut success_ids, &completed) {
                        eprintln!(
                            "[pdf-oxide] [sql-status] ERROR batch generated (last queue_id={queue_id} cif={cif}): {e:#}"
                        );
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
                let _ = file_name;
            }
            StatusJob::Failure {
                queue_id,
                cif,
                stage,
                message,
                retry,
            } => {
                eprintln!(
                    "[pdf-oxide] [sql-status] failure | stage={stage} queue_id={queue_id:?} cif={cif}: {message}"
                );
                if retry {
                    if let Some(id) = queue_id {
                        retry_ids.push(id);
                        if retry_ids.len() >= batch_cap {
                            let _ = flush_retry(&mut client, &mut retry_ids);
                        }
                    }
                }
                failed.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    if let Err(e) = flush_success(&mut client, &mut success_ids, &completed) {
        eprintln!("[pdf-oxide] [sql-status] ERROR final flush generated: {e:#}");
    }
    let _ = flush_retry(&mut client, &mut retry_ids);

    eprintln!(
        "[pdf-oxide] [sql-status] finished | completed={} failed={}",
        completed.load(Ordering::Relaxed),
        failed.load(Ordering::Relaxed)
    );
    Ok(())
}
