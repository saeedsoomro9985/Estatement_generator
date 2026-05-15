use anyhow::{Context, Result};
use futures::TryStreamExt;
use mongodb::{
    bson::{self, doc, RawDocumentBuf},
    options::{ClientOptions, FindOptions},
    Client,
};
use std::time::{Duration, Instant};

use crate::perf::MongoFetchTimings;
use crate::statement::StatementDocument;

use super::config::MongoConfig;

/// Result of a timed MongoDB load (raw fetch + BSON deserialize split).
pub struct FetchStatementsResult {
    pub documents: Vec<StatementDocument>,
    pub metrics: MongoFetchTimings,
}

/// Connect to MongoDB and return a handle to the Statements collection (raw BSON).
async fn connect_raw(config: &MongoConfig) -> Result<mongodb::Collection<RawDocumentBuf>> {
    eprintln!(
        "[pdf-oxide] connecting to MongoDB {} (db={}.{}) …",
        config.uri, config.database, config.collection
    );

    let mut options = ClientOptions::parse(&config.uri)
        .await
        .with_context(|| format!("Invalid MongoDB URI: {}", config.uri))?;
    options.connect_timeout = Some(Duration::from_secs(15));
    options.server_selection_timeout = Some(Duration::from_secs(15));

    let client = Client::with_options(options)
        .with_context(|| format!("MongoDB client init failed: {}", config.uri))?;

    eprintln!("[pdf-oxide] MongoDB connected");
    Ok(client
        .database(&config.database)
        .collection(&config.collection))
}

/// Fetch up to `limit` statement documents (newest first by `generatedAt` when present).
///
/// Timing: `metrics.connect` + `metrics.cursor_read` = network/raw BSON fetch;
/// `metrics.bson_deserialize` = serde decode into [`StatementDocument`].
pub async fn fetch_statements(
    config: &MongoConfig,
    limit: usize,
    statement_id: Option<&str>,
) -> Result<FetchStatementsResult> {
    let connect_start = Instant::now();
    let collection = connect_raw(config).await?;
    let connect_elapsed = connect_start.elapsed();

    let filter = match statement_id {
        Some(id) if !id.is_empty() => doc! {
            "$or": [
                { "statementId": id },
                { "customer.customerId": id },
            ]
        },
        _ => doc! {},
    };

    let options = FindOptions::builder()
        .limit(limit as i64)
        .build();

    eprintln!("[pdf-oxide] fetching up to {} statement(s) …", limit);

    let cursor_start = Instant::now();
    let mut cursor = collection
        .find(filter)
        .with_options(options)
        .await
        .context("MongoDB find failed")?;

    let mut raw_docs = Vec::with_capacity(limit);
    while let Some(raw) = cursor.try_next().await.context("MongoDB cursor read failed")? {
        raw_docs.push(raw);
    }
    let cursor_read_elapsed = cursor_start.elapsed();

    eprintln!(
        "[pdf-oxide] raw BSON fetch done: {} document(s) in {:.3}s (connect {:.3}s + cursor {:.3}s)",
        raw_docs.len(),
        crate::perf::secs(connect_elapsed + cursor_read_elapsed),
        crate::perf::secs(connect_elapsed),
        crate::perf::secs(cursor_read_elapsed),
    );

    let deserialize_start = Instant::now();
    let mut documents = Vec::with_capacity(raw_docs.len());
    for raw in raw_docs {
        let doc: StatementDocument = bson::from_slice(raw.as_bytes())
            .context("BSON deserialize into StatementDocument failed")?;
        documents.push(doc);
    }
    let bson_deserialize_elapsed = deserialize_start.elapsed();

    eprintln!(
        "[pdf-oxide] BSON deserialize done: {} document(s) in {:.3}s ({:.2}ms/doc)",
        documents.len(),
        crate::perf::secs(bson_deserialize_elapsed),
        bson_deserialize_elapsed.as_secs_f64() * 1000.0 / documents.len().max(1) as f64,
    );

    let metrics = MongoFetchTimings {
        connect: connect_elapsed,
        cursor_read: cursor_read_elapsed,
        bson_deserialize: bson_deserialize_elapsed,
    };

    Ok(FetchStatementsResult {
        documents,
        metrics,
    })
}

/// Fetch a single statement by `statementId` or `customer.customerId`.
pub async fn fetch_statement_by_id(
    config: &MongoConfig,
    statement_id: &str,
) -> Result<Option<StatementDocument>> {
    let mut result = fetch_statements(config, 1, Some(statement_id)).await?;
    Ok(result.documents.pop())
}
