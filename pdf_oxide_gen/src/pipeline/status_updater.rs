//! Stage 5: SQL status updates (single connection loop).

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
    status_rx: Receiver<StatusJob>,
    completed: Arc<AtomicUsize>,
    failed: Arc<AtomicUsize>,
) -> JoinHandle<Result<()>> {
    thread::Builder::new()
        .name("sql-status".into())
        .spawn(move || status_loop(sql, status_rx, completed, failed))
        .expect("spawn status updater")
}

fn status_loop(
    sql: SqlConfig,
    status_rx: Receiver<StatusJob>,
    completed: Arc<AtomicUsize>,
    failed: Arc<AtomicUsize>,
) -> Result<()> {
    let rt = Runtime::new().context("tokio runtime (status updater)")?;
    let mut client = rt.block_on(sql::connect(&sql))?;

    eprintln!("[pdf-oxide] [sql-status] started");

    while let Ok(job) = status_rx.recv() {
        let start = Instant::now();
        match &job {
            StatusJob::Success {
                queue_id,
                cif,
                file_path,
                file_name,
            } => {
                if let Err(e) = rt.block_on(sql::mark_generated(&mut client, *queue_id)) {
                    eprintln!(
                        "[pdf-oxide] [sql-status] ERROR mark_generated queue_id={queue_id} cif={cif}: {e:#}"
                    );
                    failed.fetch_add(1, Ordering::Relaxed);
                } else {
                    completed.fetch_add(1, Ordering::Relaxed);
                    if completed.load(Ordering::Relaxed) % 100 == 0 {
                        eprintln!(
                            "[pdf-oxide] [sql-status] generated={} | last queue_id={queue_id} file={file_name}",
                            completed.load(Ordering::Relaxed)
                        );
                    }
                }
                let _ = (file_path, start);
            }
            StatusJob::Failure {
                queue_id,
                cif,
                stage,
                message,
                retry,
            } => {
                eprintln!(
                    "[pdf-oxide] [sql-status] failure | stage={stage} queue_id={:?} cif={cif}: {message}",
                    queue_id
                );
                if *retry {
                    if let Some(id) = queue_id {
                        let _ = rt.block_on(sql::mark_retry_pending(&mut client, *id));
                    }
                }
                failed.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    eprintln!(
        "[pdf-oxide] [sql-status] finished | completed={} failed={}",
        completed.load(Ordering::Relaxed),
        failed.load(Ordering::Relaxed)
    );
    Ok(())
}
