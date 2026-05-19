//! `Stmt_Request_Queue` claim / complete / retry.

use anyhow::{Context, Result};
use futures::TryStreamExt;
use tiberius::QueryItem;

use super::config::SqlClient;

pub const STATUS_PENDING: i32 = 109;
pub const STATUS_PROCESSING: i32 = 103;
pub const STATUS_GENERATED: i32 = 101;

/// Max IDs per `IN (...)` batch update (fewer round-trips to SQL Server).
pub const SQL_UPDATE_CHUNK: usize = 100;

#[derive(Debug, Clone)]
pub struct QueueItem {
    pub id: i64,
    pub cif: String,
    pub frequency: String,
}

#[derive(Debug, Clone, Default)]
pub struct AdoSummary {
    pub server: Option<String>,
    pub database: Option<String>,
    pub user: Option<String>,
}

pub fn parse_ado_summary(ado: &str) -> AdoSummary {
    let mut summary = AdoSummary::default();
    for part in ado.split(';') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = v.trim().to_string();
        match key.as_str() {
            "server" | "data source" => summary.server = Some(val),
            "database" | "initial catalog" => summary.database = Some(val),
            "user id" | "uid" => summary.user = Some(val),
            _ => {}
        }
    }
    summary
}

/// Atomically claim a batch (UPDLOCK, READPAST, ROWLOCK) and set status 103.
pub async fn claim_pending_batch(
    client: &mut SqlClient,
    batch_size: i32,
    machine_id: &str,
) -> Result<Vec<QueueItem>> {
    let claim_sql = r#"
;WITH cte AS (
    SELECT TOP (@P1) Id
    FROM dbo.Stmt_Request_Queue WITH (UPDLOCK, READPAST, ROWLOCK)
    WHERE GeneratedStatus = @P2
    ORDER BY Id
)
UPDATE q
SET
    GeneratedStatus = @P3,
    ProcessingStartedAt = SYSUTCDATETIME(),
    MachineId = @P4
OUTPUT INSERTED.Id, INSERTED.CIF, INSERTED.Frequency
FROM dbo.Stmt_Request_Queue q
INNER JOIN cte ON q.Id = cte.Id;
"#;

    let mut stream = client
        .query(
            claim_sql,
            &[
                &batch_size,
                &STATUS_PENDING,
                &STATUS_PROCESSING,
                &machine_id,
            ],
        )
        .await
        .context("claim batch query failed")?;

    let mut items = Vec::new();
    while let Some(item) = stream.try_next().await.context("claim batch read")? {
        if let QueryItem::Row(row) = item {
            let id: i64 = row.get(0).unwrap_or(0);
            let cif: &str = row.get::<&str, _>(1).unwrap_or("");
            let frequency: &str = row.get::<&str, _>(2).unwrap_or("");
            items.push(QueueItem {
                id,
                cif: cif.to_string(),
                frequency: frequency.to_string(),
            });
        }
    }
    Ok(items)
}

pub async fn mark_generated(client: &mut SqlClient, queue_id: i64) -> Result<()> {
    mark_generated_batch(client, std::slice::from_ref(&queue_id)).await?;
    Ok(())
}

/// Batch-complete rows (one round-trip per chunk instead of per PDF).
pub async fn mark_generated_batch(client: &mut SqlClient, ids: &[i64]) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut total = 0usize;
    for chunk in ids.chunks(SQL_UPDATE_CHUNK) {
        let id_list = chunk
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE dbo.Stmt_Request_Queue SET GeneratedStatus = {STATUS_GENERATED}, GeneratedAt = SYSUTCDATETIME() WHERE Id IN ({id_list})"
        );
        client
            .execute(sql, &[])
            .await
            .with_context(|| format!("mark_generated_batch chunk len={}", chunk.len()))?;
        total += chunk.len();
    }
    Ok(total)
}

pub async fn mark_retry_pending(client: &mut SqlClient, queue_id: i64) -> Result<()> {
    mark_retry_pending_batch(client, std::slice::from_ref(&queue_id)).await?;
    Ok(())
}

pub async fn mark_retry_pending_batch(client: &mut SqlClient, ids: &[i64]) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut total = 0usize;
    for chunk in ids.chunks(SQL_UPDATE_CHUNK) {
        let id_list = chunk
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE dbo.Stmt_Request_Queue SET GeneratedStatus = {STATUS_PENDING}, MachineId = NULL, ProcessingStartedAt = NULL WHERE Id IN ({id_list})"
        );
        client
            .execute(sql, &[])
            .await
            .with_context(|| format!("mark_retry_pending_batch chunk len={}", chunk.len()))?;
        total += chunk.len();
    }
    Ok(total)
}
