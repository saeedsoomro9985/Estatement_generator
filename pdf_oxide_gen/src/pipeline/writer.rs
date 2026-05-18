//! Stage 4: isolated disk writers (IO only, no rendering, no MongoDB).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use crossbeam_channel::Receiver;
use std::fs;

use crate::pipeline::pdf_workers::PdfArtifact;

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
    written: std::sync::Arc<AtomicUsize>,
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
            let progress = std::sync::Arc::clone(&written);
            thread::Builder::new()
                .name(format!("writer-{id}"))
                .spawn(move || writer_loop(id, dir, rx, progress))
                .expect("spawn writer worker")
        })
        .collect()
}

fn writer_loop(
    worker_id: usize,
    output_dir: PathBuf,
    pdf_rx: Receiver<PdfArtifact>,
    written: std::sync::Arc<AtomicUsize>,
) -> Result<()> {
    while let Ok(artifact) = pdf_rx.recv() {
        let filename = format!("OXIDE-{}.pdf", safe_pdf_stem(&artifact.customer_id));
        let path = Path::new(&output_dir).join(&filename);
        fs::write(&path, &artifact.bytes).with_context(|| {
            format!(
                "Write failed: {} (writer {worker_id})",
                path.display()
            )
        })?;

        let n = written.fetch_add(1, Ordering::Relaxed) + 1;
    }
    Ok(())
}
