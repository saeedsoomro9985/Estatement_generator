use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Wall-clock timings for pipeline stages (seconds, 3 decimal places in logs).
#[derive(Debug, Clone, Copy, Default)]
pub struct StageTimings {
    /// Connect + server round-trip + raw BSON cursor read (no struct deserialize).
    pub mongo_fetch: Duration,
    /// `StatementDocument` deserialize from raw BSON.
    pub bson_deserialize: Duration,
    /// `map_statement`: Mongo model → `Customer` for PDF layout.
    pub map_statement: Duration,
}

/// Sub-timings recorded inside `fetch_statements`.
#[derive(Debug, Clone, Copy, Default)]
pub struct MongoFetchTimings {
    pub connect: Duration,
    pub cursor_read: Duration,
    pub bson_deserialize: Duration,
}

impl MongoFetchTimings {
    pub fn total_fetch(&self) -> Duration {
        self.connect + self.cursor_read
    }
}

impl StageTimings {
    pub fn from_mongo(metrics: &MongoFetchTimings) -> Self {
        Self {
            mongo_fetch: metrics.total_fetch(),
            bson_deserialize: metrics.bson_deserialize,
            map_statement: Duration::ZERO,
        }
    }

    pub fn with_map_statement(mut self, map: Duration) -> Self {
        self.map_statement = map;
        self
    }

    pub fn total(&self) -> Duration {
        self.mongo_fetch + self.bson_deserialize + self.map_statement
    }

    pub fn log_summary(&self, document_count: usize) {
        let n = document_count.max(1) as f64;
        eprintln!(
            "[pdf-oxide] timing summary (n={}) | total={:.3}s | mongo_fetch={:.3}s ({:.2}ms/doc) | bson_deserialize={:.3}s ({:.2}ms/doc) | map_statement={:.3}s ({:.2}ms/doc)",
            document_count,
            secs(self.total()),
            secs(self.mongo_fetch),
            self.mongo_fetch.as_secs_f64() * 1000.0 / n,
            secs(self.bson_deserialize),
            self.bson_deserialize.as_secs_f64() * 1000.0 / n,
            secs(self.map_statement),
            self.map_statement.as_secs_f64() * 1000.0 / n,
        );
    }

    pub fn log_mongo_detail(metrics: &MongoFetchTimings, document_count: usize) {
        eprintln!(
            "[pdf-oxide] timing mongo detail | connect={:.3}s | cursor_read={:.3}s (raw BSON, {} docs)",
            secs(metrics.connect),
            secs(metrics.cursor_read),
            document_count,
        );
    }
}

pub fn secs(d: Duration) -> f64 {
    (d.as_secs_f64() * 1000.0).round() / 1000.0
}

fn duration_from_ns(ns: u64) -> Duration {
    Duration::from_nanos(ns)
}

/// Thread-safe accumulators for streaming pipeline stages.
#[derive(Debug, Default)]
pub struct PipelineTimings {
    /// Time in `cursor.try_next()` (raw BSON from MongoDB).
    mongo_fetch_ns: AtomicU64,
    /// Time blocked on `raw_tx.send()` (downstream backpressure).
    channel_send_wait_ns: AtomicU64,
    /// BSON deserialize in decode workers.
    bson_deserialize_ns: AtomicU64,
    /// `map_statement` in decode workers.
    map_statement_ns: AtomicU64,
}

impl PipelineTimings {
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn add_mongo_fetch(&self, d: Duration) {
        self.mongo_fetch_ns
            .fetch_add(d.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn add_channel_send_wait(&self, d: Duration) {
        self.channel_send_wait_ns
            .fetch_add(d.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn add_bson_deserialize(&self, d: Duration) {
        self.bson_deserialize_ns
            .fetch_add(d.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn add_map_statement(&self, d: Duration) {
        self.map_statement_ns
            .fetch_add(d.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn mongo_fetch(&self) -> Duration {
        duration_from_ns(self.mongo_fetch_ns.load(Ordering::Relaxed))
    }

    pub fn channel_send_wait(&self) -> Duration {
        duration_from_ns(self.channel_send_wait_ns.load(Ordering::Relaxed))
    }

    pub fn bson_deserialize(&self) -> Duration {
        duration_from_ns(self.bson_deserialize_ns.load(Ordering::Relaxed))
    }

    pub fn map_statement(&self) -> Duration {
        duration_from_ns(self.map_statement_ns.load(Ordering::Relaxed))
    }

    /// BSON decode + map (decode stage total).
    pub fn decode_total(&self) -> Duration {
        self.bson_deserialize() + self.map_statement()
    }

    pub fn log_fetch_vs_decode(&self, document_count: usize) {
        let n = document_count.max(1) as f64;
        let fetch = self.mongo_fetch();
        let decode = self.decode_total();
        let send_wait = self.channel_send_wait();
        eprintln!(
            "[pdf-oxide] timing fetch vs decode (n={}) | mongo_fetch={:.3}s ({:.2}ms/doc) | decode={:.3}s ({:.2}ms/doc) [bson={:.3}s map={:.3}s] | channel_send_wait={:.3}s",
            document_count,
            secs(fetch),
            fetch.as_secs_f64() * 1000.0 / n,
            secs(decode),
            decode.as_secs_f64() * 1000.0 / n,
            secs(self.bson_deserialize()),
            secs(self.map_statement()),
            secs(send_wait),
        );
    }
}
