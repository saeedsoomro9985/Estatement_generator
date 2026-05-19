//! Stage 4: isolated disk writers (IO only).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};
use std::fs;

use crate::pipeline::pdf_workers::PdfArtifact;
use crate::pipeline::status_updater::StatusJob;

fn safe_pdf_stem(id: &str) -> String {
    const INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let stem: String = id
        .chars()
        .map(|c| if INVALID.contains(&c) { '_' } else { c })
        .collect();
    if stem.is_empty() {
        "unknown".to_string()
    } else {
        stem
    }
}

pub fn spawn_writer_workers(
    worker_count: usize,
    output_dir: PathBuf,
    pdf_rx: Receiver<PdfArtifact>,
    written: Arc<AtomicUsize>,
    status_tx: Sender<StatusJob>,
) -> Vec<JoinHandle<Result<()>>> {
    let display_path = fs::canonicalize(&output_dir).unwrap_or_else(|_| output_dir.clone());
    eprintln!(
        "[pdf-oxide] [writer] output → {}",
        display_path.display()
    );

    (0..worker_count)
        .map(|id| {
            let rx = pdf_rx.clone();
            let dir = output_dir.clone();
            let progress = Arc::clone(&written);
            let stx = status_tx.clone();
            thread::Builder::new()
                .name(format!("writer-{id}"))
                .spawn(move || writer_loop(id, dir, rx, progress, stx))
                .expect("spawn writer worker")
        })
        .collect()
}

fn writer_loop(
    worker_id: usize,
    output_dir: PathBuf,
    pdf_rx: Receiver<PdfArtifact>,
    written: Arc<AtomicUsize>,
    status_tx: Sender<StatusJob>,
) -> Result<()> {
    while let Ok(artifact) = pdf_rx.recv() {
        let filename = format!("OXIDE-{}.pdf", safe_pdf_stem(&artifact.customer_id));
        let path = Path::new(&output_dir).join(&filename);

        if let Err(e) = fs::write(&path, &artifact.bytes) {
            let msg = format!("{e:#}");
            eprintln!(
                "[pdf-oxide] [writer] ERROR queue_id={} path={}: {msg}",
                artifact.queue_id,
                path.display()
            );
            let _ = status_tx.send(StatusJob::Failure {
                queue_id: Some(artifact.queue_id),
                cif: artifact.cif.clone(),
                stage: "disk_write".into(),
                message: msg,
                retry: true,
            });
            continue;
        }

        let n = written.fetch_add(1, Ordering::Relaxed) + 1;
        if n == 1 {
            eprintln!("[pdf-oxide] [writer] first PDF: {}", path.display());
        }

        let file_path = fs::canonicalize(&path)
            .unwrap_or_else(|_| path.clone())
            .display()
            .to_string();

        if status_tx
            .send(StatusJob::Success {
                queue_id: artifact.queue_id,
                cif: artifact.cif,
                file_path,
                file_name: filename,
            })
            .is_err()
        {
            eprintln!(
                "[pdf-oxide] [writer] WARN: status channel closed after write (queue_id={})",
                artifact.queue_id
            );
        }
    }
    Ok(())
}
