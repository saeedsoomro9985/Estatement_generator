//! SQL Server queue access (claim, complete, retry).

pub mod config;
mod queue;

pub use config::{connect, test_connection, SqlClient, SqlConfig};
pub use queue::{
    claim_pending_batch, mark_generated, mark_generated_batch, mark_retry_pending,
    mark_retry_pending_batch, SQL_UPDATE_CHUNK,
    QueueItem, STATUS_GENERATED, STATUS_PENDING, STATUS_PROCESSING,
};
