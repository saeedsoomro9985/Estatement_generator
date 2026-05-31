//! Stage 3: CPU-bound PDF rendering pool (no MongoDB, no SQL).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};

use crate::pipeline::mongo_fetch::EnrichedWork;
use crate::render::render_pdf;

/// Rendered PDF bytes + queue metadata for writer / SQL status.
pub struct PdfArtifact {
    pub queue_id: i64,
    pub cif: String,
    pub customer_id: String,
    pub bytes: Vec<u8>,
}

pub fn spawn_pdf_workers(
    worker_count: usize,
    enriched_rx: Receiver<EnrichedWork>,
    pdf_tx: Sender<PdfArtifact>,
    rendered: std::sync::Arc<AtomicUsize>,
) -> Vec<JoinHandle<Result<()>>> {
    (0..worker_count)
        .map(|id| {
            let rx = enriched_rx.clone();
            let tx = pdf_tx.clone();
            let progress = std::sync::Arc::clone(&rendered);
            thread::Builder::new()
                .name(format!("pdf-{id}"))
                .spawn(move || pdf_worker_loop(id, rx, tx, progress))
                .expect("spawn pdf worker")
        })
        .collect()
}

fn pdf_worker_loop(
    worker_id: usize,
    enriched_rx: Receiver<EnrichedWork>,
    pdf_tx: Sender<PdfArtifact>,
    rendered: std::sync::Arc<AtomicUsize>,
) -> Result<()> {
    while let Ok(work) = enriched_rx.recv() {
        let customer_id = work.customer.customer_id.clone();
        let queue_id = work.queue.id;
        let cif = work.queue.cif.clone();
        let bytes = render_pdf(&work.customer).with_context(|| {
            format!("PDF render failed queue_id={queue_id} cif={cif} (pdf worker {worker_id})")
        })?;

        pdf_tx
            .send(PdfArtifact {
                queue_id,
                cif,
                customer_id,
                bytes,
            })
            .map_err(|_| anyhow::anyhow!("PDF channel closed (writer stage exited early)"))?;

        let n = rendered.fetch_add(1, Ordering::Relaxed) + 1;
        if n % 100 == 0 {
            eprintln!("[pdf-oxide] [pdf] rendered {n} PDF(s)");
        }
    }
    Ok(())
}
