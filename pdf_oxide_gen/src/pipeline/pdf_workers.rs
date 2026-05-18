//! Stage 3: CPU-bound PDF rendering pool (no MongoDB, no disk IO).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};

use crate::pipeline::decode::DecodedCustomer;
use crate::render::render_pdf;

/// Rendered PDF bytes handed off to the writer stage.
pub struct PdfArtifact {
    pub customer_id: String,
    pub bytes: Vec<u8>,
}

pub fn spawn_pdf_workers(
    worker_count: usize,
    customer_rx: Receiver<DecodedCustomer>,
    pdf_tx: Sender<PdfArtifact>,
    rendered: std::sync::Arc<AtomicUsize>,
) -> Vec<JoinHandle<Result<()>>> {
    (0..worker_count)
        .map(|id| {
            let rx = customer_rx.clone();
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
    customer_rx: Receiver<DecodedCustomer>,
    pdf_tx: Sender<PdfArtifact>,
    rendered: std::sync::Arc<AtomicUsize>,
) -> Result<()> {
    while let Ok(customer) = customer_rx.recv() {
        let customer_id = customer.id.clone();
        let bytes = render_pdf(&customer).with_context(|| {
            format!("PDF render failed for customer id={customer_id} (pdf worker {worker_id})")
        })?;

        pdf_tx
            .send(PdfArtifact { customer_id, bytes })
            .map_err(|_| anyhow::anyhow!("PDF channel closed (writer stage exited early)"))?;

        let n = rendered.fetch_add(1, Ordering::Relaxed) + 1;
        if n % 100 == 0 {
            eprintln!("[pdf-oxide] [pdf] rendered {n} PDF(s)");
        }
    }
    Ok(())
}
