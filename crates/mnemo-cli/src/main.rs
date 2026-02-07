use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};
use tokio::sync::Notify;

use mnemo_core::embedding::openai::OpenAiEmbedding;
use mnemo_core::embedding::{EmbeddingProvider, NoopEmbedding};
use mnemo_core::encryption::ContentEncryption;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::index::VectorIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::search::FullTextIndex;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;
use mnemo_mcp::server::MnemoServer;

#[derive(Parser)]
#[command(name = "mnemo", about = "MCP-native memory database for AI agents")]
struct Cli {
    /// Path to the database file
    #[arg(long, default_value = "mnemo.db", env = "MNEMO_DB_PATH")]
    db_path: PathBuf,

    /// OpenAI API key for embeddings
    #[arg(long, env = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,

    /// Embedding model name
    #[arg(long, default_value = "text-embedding-3-small", env = "MNEMO_EMBEDDING_MODEL")]
    embedding_model: String,

    /// Embedding dimensions
    #[arg(long, default_value = "1536", env = "MNEMO_DIMENSIONS")]
    dimensions: usize,

    /// Default agent ID
    #[arg(long, default_value = "default", env = "MNEMO_AGENT_ID")]
    agent_id: String,

    /// Default organization ID
    #[arg(long, env = "MNEMO_ORG_ID")]
    org_id: Option<String>,

    /// Path to ONNX embedding model (uses local inference instead of OpenAI)
    #[arg(long, env = "MNEMO_ONNX_MODEL_PATH")]
    onnx_model_path: Option<String>,

    /// PostgreSQL connection URL (enables PostgreSQL backend instead of DuckDB)
    #[arg(long, env = "MNEMO_POSTGRES_URL")]
    postgres_url: Option<String>,

    /// REST API port (starts an HTTP server alongside MCP stdio)
    #[arg(long, env = "MNEMO_REST_PORT")]
    rest_port: Option<u16>,

    /// Idle timeout in seconds — auto-shutdown after no requests (0 = disabled)
    #[arg(long, default_value = "0", env = "MNEMO_IDLE_TIMEOUT")]
    idle_timeout_seconds: u64,

    /// AES-256-GCM encryption key (64-char hex string) for at-rest content encryption
    #[arg(long, env = "MNEMO_ENCRYPTION_KEY")]
    encryption_key: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mnemo=info".parse()?)
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    // Initialize embedding provider (ONNX > OpenAI > Noop)
    let embedding: Arc<dyn EmbeddingProvider> = if let Some(ref onnx_path) = cli.onnx_model_path {
        tracing::info!("Using ONNX local embeddings from {}", onnx_path);
        Arc::new(mnemo_core::embedding::onnx::OnnxEmbedding::new(onnx_path, cli.dimensions)?)
    } else if let Some(api_key) = cli.openai_api_key {
        tracing::info!("Using OpenAI embeddings ({})", cli.embedding_model);
        Arc::new(OpenAiEmbedding::new(
            api_key,
            cli.embedding_model,
            cli.dimensions,
        ))
    } else {
        tracing::warn!("No OPENAI_API_KEY set, using noop embeddings (semantic search will not work)");
        Arc::new(NoopEmbedding::new(cli.dimensions))
    };

    // Build engine based on backend selection
    // Keep a reference to the DuckDB vector index for shutdown save
    #[allow(unused_assignments)]
    let mut duckdb_index: Option<Arc<UsearchIndex>> = None;
    let engine = if let Some(_pg_url) = &cli.postgres_url {
        #[cfg(feature = "postgres")]
        {
            let pg_storage = Arc::new(
                mnemo_postgres::PgStorage::connect(_pg_url, cli.dimensions).await?
            );
            let pg_index = Arc::new(mnemo_postgres::PgVectorIndex::new());
            tracing::info!("Using PostgreSQL backend");
            let mut eng = MnemoEngine::new(
                pg_storage,
                pg_index,
                embedding,
                cli.agent_id.clone(),
                cli.org_id.clone(),
            );
            if let Some(ref key_hex) = cli.encryption_key {
                let enc = ContentEncryption::from_hex(key_hex)?;
                eng = eng.with_encryption(Arc::new(enc));
                tracing::info!("At-rest encryption enabled");
            }
            Arc::new(eng)
        }
        #[cfg(not(feature = "postgres"))]
        {
            return Err("PostgreSQL support not enabled. Rebuild with --features postgres".into());
        }
    } else {
        // DuckDB backend (default)
        let storage = Arc::new(DuckDbStorage::open(&cli.db_path)?);
        tracing::info!("Database opened at {:?}", cli.db_path);

        let index = Arc::new(UsearchIndex::new(cli.dimensions)?);

        // Load existing index if available
        let index_path = cli.db_path.with_extension("usearch");
        if index_path.exists() {
            index.load(&index_path)?;
            tracing::info!("Loaded vector index ({} vectors)", index.len());
        }

        // Initialize full-text index
        let ft_path = cli.db_path.with_extension("tantivy");
        let full_text = Arc::new(TantivyFullTextIndex::new(&ft_path)?);
        tracing::info!("Full-text index ready at {:?} ({} docs)", ft_path, full_text.len());

        // Keep a clone of the actual index for shutdown save
        duckdb_index = Some(index.clone());

        let mut eng = MnemoEngine::new(
                storage,
                index.clone(),
                embedding,
                cli.agent_id.clone(),
                cli.org_id.clone(),
            )
            .with_full_text(full_text.clone());
        if let Some(ref key_hex) = cli.encryption_key {
            let enc = ContentEncryption::from_hex(key_hex)?;
            eng = eng.with_encryption(Arc::new(enc));
            tracing::info!("At-rest encryption enabled");
        }
        Arc::new(eng)
    };

    // Optionally start REST API server
    #[cfg(feature = "rest")]
    if let Some(port) = cli.rest_port {
        let rest_engine = engine.clone();
        tokio::spawn(async move {
            let app = mnemo_rest::router(rest_engine);
            match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
                Ok(listener) => {
                    tracing::info!("REST API listening on 0.0.0.0:{port}");
                    if let Err(e) = axum::serve(listener, app).await {
                        tracing::error!("REST server failed: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to bind REST port {port}: {e}");
                }
            }
        });
    }

    // Shared activity tracker for idle timeout
    let activity_tracker = if cli.idle_timeout_seconds > 0 {
        Some(Arc::new(AtomicU64::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )))
    } else {
        None
    };

    // Shared shutdown signal
    let shutdown_notify = Arc::new(Notify::new());

    // Start idle timeout watchdog (for scale-to-zero)
    if let Some(ref tracker) = activity_tracker {
        let timeout = cli.idle_timeout_seconds;
        let watchdog_tracker = tracker.clone();
        let watchdog_engine = engine.clone();
        let watchdog_shutdown = shutdown_notify.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                let last = watchdog_tracker.load(Ordering::Relaxed);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                if now - last > timeout {
                    tracing::info!(
                        "Idle timeout reached ({timeout}s), shutting down for scale-to-zero"
                    );
                    // Checkpoint before exit so state can be restored on next start
                    match watchdog_engine.checkpoint(
                        mnemo_core::query::checkpoint::CheckpointRequest {
                            thread_id: "__shutdown__".to_string(),
                            agent_id: None,
                            branch_name: Some("main".to_string()),
                            state_snapshot: serde_json::json!({"reason": "idle_timeout"}),
                            label: Some("auto-shutdown".to_string()),
                            metadata: None,
                        }
                    ).await {
                        Ok(resp) => tracing::info!("Shutdown checkpoint created: {}", resp.id),
                        Err(e) => tracing::warn!("Failed to create shutdown checkpoint: {e}"),
                    }
                    watchdog_shutdown.notify_one();
                    return;
                }
            }
        });

        tracing::info!("Idle timeout watchdog enabled: {timeout}s");
    }

    // Signal handler for graceful shutdown (Ctrl+C / SIGTERM)
    let signal_shutdown = shutdown_notify.clone();
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to listen for Ctrl+C: {e}");
            return;
        }
        tracing::info!("Received shutdown signal");
        signal_shutdown.notify_one();
    });

    // Create and start MCP server
    let mut server = MnemoServer::new(engine);
    if let Some(ref tracker) = activity_tracker {
        server = server.with_activity_tracker(tracker.clone());
    }
    tracing::info!("Starting Mnemo MCP server on stdio");

    let service = server.serve(stdio()).await?;

    // Wait for either MCP service to end or a shutdown signal
    tokio::select! {
        result = service.waiting() => {
            if let Err(e) = result {
                tracing::error!("MCP service error: {e}");
            }
        }
        _ = shutdown_notify.notified() => {
            tracing::info!("Shutdown initiated, saving state...");
        }
    }

    // Save DuckDB vector index on shutdown (using the actual populated index)
    if let Some(ref index) = duckdb_index {
        let index_path = cli.db_path.with_extension("usearch");
        tracing::info!("Saving vector index ({} vectors)...", index.len());
        if let Err(e) = index.save(&index_path) {
            tracing::error!("Failed to save vector index: {}", e);
        }
    }

    Ok(())
}
