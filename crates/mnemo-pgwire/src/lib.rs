//! PostgreSQL wire protocol server for Mnemo.
//!
//! Exposes Mnemo's memory database through the PostgreSQL wire protocol,
//! allowing SQL-native clients (psql, pgAdmin, any PostgreSQL driver) to
//! query memories using familiar SQL syntax.
//!
//! # Supported SQL subset
//!
//! - `SELECT * FROM memories WHERE agent_id = '...' LIMIT n`
//! - `INSERT INTO memories (content, importance, ...) VALUES (...)`
//! - `DELETE FROM memories WHERE id = '...'`
//!
//! # Architecture
//!
//! The server accepts TCP connections and speaks the PostgreSQL wire protocol
//! (startup, query, parse/bind/execute extended protocol). Queries are parsed
//! and mapped to Mnemo engine operations:
//!
//! - `SELECT` → `engine.recall()`
//! - `INSERT` → `engine.remember()`
//! - `DELETE` → `engine.forget()`

pub mod parser;
pub mod server;

use std::sync::Arc;
use mnemo_core::query::MnemoEngine;

/// Configuration for the pgwire server.
#[derive(Debug, Clone)]
pub struct PgWireConfig {
    /// TCP bind address (e.g., "0.0.0.0:5433")
    pub bind_addr: String,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Default agent ID for connections without explicit agent context
    pub default_agent_id: String,
}

impl Default for PgWireConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:5433".to_string(),
            max_connections: 100,
            default_agent_id: "default".to_string(),
        }
    }
}

/// Start the pgwire server.
///
/// Listens on the configured address and accepts PostgreSQL wire protocol
/// connections. Each connection is handled in a separate tokio task.
///
/// # Example
///
/// ```no_run
/// # use std::sync::Arc;
/// # use mnemo_pgwire::{PgWireConfig, start_server};
/// # use mnemo_core::query::MnemoEngine;
/// # async fn run(engine: Arc<MnemoEngine>) {
/// let config = PgWireConfig::default();
/// start_server(engine, config).await.unwrap();
/// # }
/// ```
pub async fn start_server(
    engine: Arc<MnemoEngine>,
    config: PgWireConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("pgwire server listening on {}", config.bind_addr);

    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.max_connections));

    loop {
        let (stream, addr) = listener.accept().await?;
        tracing::debug!("pgwire connection from {addr}");

        let engine = engine.clone();
        let config = config.clone();
        let permit = semaphore.clone().acquire_owned().await?;

        tokio::spawn(async move {
            if let Err(e) = server::handle_connection(stream, engine, &config).await {
                tracing::warn!("pgwire connection error from {addr}: {e}");
            }
            drop(permit);
        });
    }
}
