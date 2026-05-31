//! Legacy PDF workers (Mongo-direct pipeline, DecodedCustomer input).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};

use crate::pipeline::decode::DecodedCustomer;
use crate::pipeline::pdf_workers::PdfArtifact;
use crate::render::render_pdf;

pub fn spawn_legacy_pdf_workers(
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
                .name(format!("pdf-legacy-{id}"))
                .spawn(move || legacy_pdf_loop(id, rx, tx, progress))
                .expect("spawn legacy pdf worker")
        })
        .collect()
}

fn legacy_pdf_loop(
    worker_id: usize,
    customer_rx: Receiver<DecodedCustomer>,
    pdf_tx: Sender<PdfArtifact>,
    rendered: std::sync::Arc<AtomicUsize>,
) -> Result<()> {
    while let Ok(customer) = customer_rx.recv() {
        let customer_id = customer.customer_id.clone();
        let bytes = render_pdf(&customer).with_context(|| {
            format!("PDF render failed id={customer_id} (worker {worker_id})")
        })?;
        pdf_tx
            .send(PdfArtifact {
                queue_id: 0,
                cif: customer_id.clone(),
                customer_id,
                bytes,
            })
            .map_err(|_| anyhow::anyhow!("PDF channel closed"))?;
        let n = rendered.fetch_add(1, Ordering::Relaxed) + 1;
        if n % 100 == 0 {
            eprintln!("[pdf-oxide] [pdf] rendered {n} PDF(s)");
        }
    }
    Ok(())
}
