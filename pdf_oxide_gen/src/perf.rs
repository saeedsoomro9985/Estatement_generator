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
