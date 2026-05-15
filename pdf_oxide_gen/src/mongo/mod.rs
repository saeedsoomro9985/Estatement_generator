pub mod config;
pub mod repository;

pub use config::MongoConfig;
pub use repository::{fetch_statement_by_id, fetch_statements, FetchStatementsResult};
