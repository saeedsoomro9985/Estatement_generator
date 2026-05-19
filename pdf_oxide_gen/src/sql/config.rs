//! SQL Server connection settings (ADO.NET string from Node / CLI).

use anyhow::{Context, Result};
use tiberius::{Client, Config, EncryptionLevel};
use tokio_util::compat::TokioAsyncWriteCompatExt;

#[derive(Debug, Clone)]
pub struct SqlConfig {
    pub connection_string: String,
    pub machine_id: String,
}

impl SqlConfig {
    pub fn new(connection_string: impl Into<String>, machine_id: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
            machine_id: machine_id.into(),
        }
    }

    pub fn log_target(&self) {
        let info = crate::sql::queue::parse_ado_summary(&self.connection_string);
        eprintln!(
            "[pdf-oxide] [sql] target | server={} | database={} | user={} | machine_id={}",
            info.server.as_deref().unwrap_or("?"),
            info.database.as_deref().unwrap_or("?"),
            info.user.as_deref().unwrap_or("?"),
            self.machine_id,
        );
    }
}

pub type SqlClient = Client<tokio_util::compat::Compat<tokio::net::TcpStream>>;

pub async fn connect(cfg: &SqlConfig) -> Result<SqlClient> {
    cfg.log_target();
    let mut tiberius_cfg = Config::from_ado_string(&cfg.connection_string)
        .context("Invalid MSSQL ADO connection string")?;
    let _ = tiberius_cfg.trust_cert();
    tiberius_cfg.encryption(EncryptionLevel::NotSupported);

    let addr = tiberius_cfg.get_addr();
    eprintln!("[pdf-oxide] [sql] TCP → {:?}", addr);
    let tcp = tokio::net::TcpStream::connect(addr.clone())
        .await
        .with_context(|| format!("MSSQL TCP failed ({addr:?})"))?;
    tcp.set_nodelay(true)?;
    let client = Client::connect(tiberius_cfg, tcp.compat_write())
        .await
        .context("MSSQL login failed")?;
    eprintln!("[pdf-oxide] [sql] connected");
    Ok(client)
}

pub async fn test_connection(cfg: &SqlConfig) -> Result<()> {
    let mut client = connect(cfg).await?;
    client
        .execute("SELECT 1", &[])
        .await
        .context("MSSQL test query failed")?;
    eprintln!("[pdf-oxide] [sql] test query OK");
    Ok(())
}
