use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use clap::{Parser, Subcommand};
use rmcp::{ServiceExt, transport::stdio};
use tokio::sync::Notify;

mod attest;
mod commands;
mod lease;
mod manifest;
mod safe_spawn;

use mnemo_core::anomaly::outlier::train_baseline;
use mnemo_core::embedding::openai::OpenAiEmbedding;
use mnemo_core::embedding::{EmbeddingProvider, NoopEmbedding};
use mnemo_core::encryption::ContentEncryption;
use mnemo_core::index::VectorIndex;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::search::FullTextIndex;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::StorageBackend;
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
    #[arg(
        long,
        default_value = "text-embedding-3-small",
        env = "MNEMO_EMBEDDING_MODEL"
    )]
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

    /// Interval in seconds between TTL sweeps (0 = disabled). A sweep hard-deletes
    /// every memory whose `expires_at` is in the past and emits MemoryExpired
    /// audit events.
    #[arg(long, default_value = "0", env = "MNEMO_TTL_SWEEP_INTERVAL")]
    ttl_sweep_interval_seconds: u64,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Manage the per-agent embedding-space baseline used by the z-score
    /// outlier detector (v0.3.3, Task A).
    Baseline(BaselineArgs),
    /// Start the MCP STDIO server in hardened mode using a TOML manifest
    /// (v0.4.0-rc3 Task B2).
    ///
    /// Defends against the OX-MCP "exfiltrate-then-act" disclosure
    /// (2026-04-24): refuses inherited secrets, JSON-injection argv, and
    /// untrusted parent processes before any engine state is touched. All
    /// privileged knobs come from the TOML manifest — env vars and
    /// command-line flags cannot grant capabilities.
    McpServer(McpServerArgs),
    /// Replay a JSONL dataset of `{query, expected}` rows against an
    /// in-memory engine and emit a per-row latency / top-k report
    /// (v0.4.0-rc3 Task B6).
    ///
    /// The bundled dataset at `crates/mnemo-core/benches/data/longmemeval_m.jsonl`
    /// is the default when `--dataset` is omitted. Used to compare
    /// configuration sweeps (provenance on/off, recency half-life,
    /// hybrid weights) against a fixed prompt set.
    Eval(EvalArgs),
}

#[derive(clap::Args)]
struct BaselineArgs {
    /// Train and persist a baseline from every non-deleted memory for this agent.
    #[arg(long)]
    train: bool,

    /// Agent ID to train or inspect the baseline for. Falls back to
    /// `--agent-id` / `MNEMO_AGENT_ID` when omitted.
    #[arg(long)]
    agent_id: Option<String>,

    /// Maximum records to load when training (defaults to `MAX_BATCH_QUERY_LIMIT`).
    #[arg(long, default_value = "10000")]
    limit: usize,
}

#[derive(clap::Args)]
struct McpServerArgs {
    /// Path to the TOML manifest carrying every privileged knob.
    #[arg(long)]
    manifest: PathBuf,
}

#[derive(clap::Args)]
struct EvalArgs {
    /// Path to a JSONL dataset of `{id, content, query, expected}` rows.
    /// Defaults to the bundled LongMemEval_M sample.
    #[arg(long)]
    dataset: Option<PathBuf>,
    /// Where to write per-row results as JSONL. Defaults to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Recall limit per query.
    #[arg(long, default_value = "5")]
    limit: usize,
    /// Request a provenance receipt on every recall.
    #[arg(long)]
    with_provenance: bool,
    /// HMAC key (hex, >=32 bytes) for the provenance signer. Required
    /// when `--with-provenance` is set.
    #[arg(long)]
    provenance_key_hex: Option<String>,
    /// Recall strategy ("semantic", "hybrid", "lexical").
    #[arg(long, default_value = "hybrid")]
    strategy: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("mnemo=info".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    // Dispatch one-shot subcommands before any server setup.
    match &cli.command {
        Some(Command::Baseline(args)) => return run_baseline(&cli, args).await,
        Some(Command::McpServer(args)) => return run_mcp_server(&cli, args).await,
        Some(Command::Eval(args)) => return run_eval(&cli, args).await,
        None => {}
    }

    // Initialize embedding provider (ONNX > OpenAI > Noop)
    let embedding: Arc<dyn EmbeddingProvider> = if let Some(ref onnx_path) = cli.onnx_model_path {
        tracing::info!("Using ONNX local embeddings from {}", onnx_path);
        Arc::new(mnemo_core::embedding::onnx::OnnxEmbedding::new(
            onnx_path,
            cli.dimensions,
        )?)
    } else if let Some(api_key) = cli.openai_api_key {
        tracing::info!("Using OpenAI embeddings ({})", cli.embedding_model);
        Arc::new(OpenAiEmbedding::new(
            api_key,
            cli.embedding_model,
            cli.dimensions,
        ))
    } else {
        tracing::warn!(
            "No OPENAI_API_KEY set, using noop embeddings (semantic search will not work)"
        );
        Arc::new(NoopEmbedding::new(cli.dimensions))
    };

    // Build engine based on backend selection
    // Keep a reference to the DuckDB vector index for shutdown save
    #[allow(unused_assignments)]
    let mut duckdb_index: Option<Arc<UsearchIndex>> = None;
    let engine = if let Some(_pg_url) = &cli.postgres_url {
        #[cfg(feature = "postgres")]
        {
            let pg_storage =
                Arc::new(mnemo_postgres::PgStorage::connect(_pg_url, cli.dimensions).await?);
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
        tracing::info!(
            "Full-text index ready at {:?} ({} docs)",
            ft_path,
            full_text.len()
        );

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
                    match watchdog_engine
                        .checkpoint(mnemo_core::query::checkpoint::CheckpointRequest {
                            thread_id: "__shutdown__".to_string(),
                            agent_id: None,
                            branch_name: Some("main".to_string()),
                            state_snapshot: serde_json::json!({"reason": "idle_timeout"}),
                            label: Some("auto-shutdown".to_string()),
                            metadata: None,
                        })
                        .await
                    {
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

    // Start TTL sweeper that hard-deletes expired memories on a fixed cadence.
    // Disabled when ttl_sweep_interval_seconds == 0.
    if cli.ttl_sweep_interval_seconds > 0 {
        let ttl_interval = cli.ttl_sweep_interval_seconds;
        let ttl_engine = engine.clone();
        let ttl_shutdown = shutdown_notify.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(ttl_interval));
            // Skip the immediate first tick so startup isn't surprised by a sweep.
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match ttl_engine.run_ttl_sweep().await {
                            Ok(report) if report.swept_count > 0 || !report.errors.is_empty() => {
                                tracing::info!(
                                    swept = report.swept_count,
                                    errors = report.errors.len(),
                                    "TTL sweep complete"
                                );
                            }
                            Ok(_) => {}
                            Err(e) => tracing::warn!("TTL sweep failed: {e}"),
                        }
                    }
                    _ = ttl_shutdown.notified() => return,
                }
            }
        });
        tracing::info!("TTL sweeper enabled (every {ttl_interval}s)");
    }

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

/// Handle `mnemo baseline --train --agent-id <id>` (v0.3.3 Task A).
///
/// Loads every non-deleted memory for the agent from DuckDB, computes
/// per-dimension mean + diagonal variance over the records that carry an
/// embedding, and persists the result to the `embedding_baseline` table.
/// Subsequent `remember` calls with
/// `PoisoningPolicy::with_outlier_threshold(z)` set will be scored
/// against this baseline.
async fn run_baseline(cli: &Cli, args: &BaselineArgs) -> Result<(), Box<dyn std::error::Error>> {
    if !args.train {
        return Err(
            "baseline: nothing to do — pass `--train` to train and persist a baseline".into(),
        );
    }
    let agent_id = args
        .agent_id
        .clone()
        .unwrap_or_else(|| cli.agent_id.clone());
    if agent_id.is_empty() {
        return Err("baseline: --agent-id is required (or set MNEMO_AGENT_ID)".into());
    }

    tracing::info!(
        agent = %agent_id,
        db = ?cli.db_path,
        "training embedding baseline"
    );

    let storage = Arc::new(DuckDbStorage::open(&cli.db_path)?);
    let filter = mnemo_core::storage::MemoryFilter {
        agent_id: Some(agent_id.clone()),
        ..Default::default()
    };
    let records = storage.list_memories(&filter, args.limit, 0).await?;
    let with_emb = records.iter().filter(|r| r.embedding.is_some()).count();
    tracing::info!(
        total = records.len(),
        with_embedding = with_emb,
        "loaded records"
    );

    let Some(baseline) = train_baseline(&agent_id, &records) else {
        return Err(format!(
            "baseline: not enough embedded records to train for agent {agent_id} (found {with_emb})"
        )
        .into());
    };

    storage
        .insert_or_update_embedding_baseline(&baseline)
        .await?;
    println!(
        "baseline trained for agent '{}' — n={} d={} updated_at={}",
        baseline.agent_id,
        baseline.n,
        baseline.mu.len(),
        baseline.updated_at
    );
    Ok(())
}

/// Handle `mnemo mcp-server --manifest <path>` (v0.4.0-rc3 Task B2).
///
/// Runs the safe-spawn gauntlet against the OX-MCP threat model
/// (2026-04-24) BEFORE constructing any engine state, then starts the
/// existing MCP STDIO server. The lease store is allocated here so a
/// future change to the MCP tools layer can require lease tokens for
/// privileged operations without re-plumbing the binary.
async fn run_mcp_server(cli: &Cli, args: &McpServerArgs) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = manifest::Manifest::load(&args.manifest)?;
    tracing::info!(
        manifest = ?args.manifest,
        allowed_tools = ?manifest.allowed_tools,
        allowed_parents = ?manifest.allowed_parents,
        lease_ttl_seconds = manifest.lease_ttl_seconds,
        "manifest loaded"
    );

    // Gauntlet step 1: refuse inherited secrets (override with
    // `MNEMO_REJECT_INHERITED_SECRETS=0` for opt-out testing only).
    let reject_secrets = std::env::var("MNEMO_REJECT_INHERITED_SECRETS").as_deref() != Ok("0");
    safe_spawn::check_inherited_secrets(std::env::vars(), reject_secrets)?;

    // Gauntlet step 2: refuse JSON-injection-style argv. Operators must
    // express config via the manifest, not via `--config`/`-c`.
    let argv: Vec<String> = std::env::args().collect();
    safe_spawn::check_args_pattern(&argv)?;

    // Gauntlet step 3: refuse untrusted parent processes when stdin is
    // not a TTY. Parent basename comes from `MNEMO_PARENT_BASENAME` so
    // the operator's launcher controls the trust assertion (we avoid
    // pulling in libc / sysctl just to read /proc).
    let parent_basename = std::env::var("MNEMO_PARENT_BASENAME").ok();
    let has_tty = std::io::stdin().is_terminal();
    safe_spawn::check_parent_process(
        parent_basename.as_deref(),
        has_tty,
        &manifest.allowed_parents,
    )?;
    tracing::info!(
        has_tty,
        parent = parent_basename.as_deref().unwrap_or("<unknown>"),
        "safe-spawn gauntlet passed"
    );

    // The manifest pins the agent set the operator has approved for
    // this binary. An empty set means "any agent", matching the
    // permissive default the test suite exercises.
    if !manifest.allowed_agents.is_empty() && !manifest.allowed_agents.contains(&cli.agent_id) {
        return Err(format!(
            "refused to start: agent_id {:?} is not in manifest.allowed_agents (got {:?})",
            cli.agent_id, manifest.allowed_agents
        )
        .into());
    }

    // The manifest's `audit_log_path` is the destination for future
    // append-only audit exports (see B4). It is logged here so an
    // operator running the binary can see exactly what the manifest
    // committed them to before any traffic flows.
    tracing::info!(
        audit_log_path = ?manifest.audit_log_path,
        "audit log destination configured"
    );

    // Load the HMAC keystore the manifest points at and attach a
    // `ProvenanceSigner` to the engine. With the signer attached, every
    // `recall(..., with_provenance=true)` returns a verifiable receipt
    // (B1) — and crucially, the key material reaches the engine via a
    // chmod-restricted file, never via env or argv.
    let keystore = manifest::Keystore::load(&manifest.keystore_path)?;
    let key_bytes = keystore.key_bytes()?;
    let signer = mnemo_core::provenance::ProvenanceSigner::new(&keystore.key_id, &key_bytes);
    tracing::info!(
        key_id = %signer.key_id(),
        "provenance signer attached"
    );

    // v0.4.0 (P0-1) — load the optional tool-catalog pin and build
    // an attestor. The actual attestation against rmcp's advertised
    // tool list happens in the MCP boot path (a separate follow-up
    // wires it into mnemo-mcp's ServerHandler::list_tools — keeping
    // the attestor here ensures the manifest's pin is parsed and
    // validated even before that wiring lands, so a malformed pin
    // refuses startup rather than silently passing through).
    let tool_attestor: Option<attest::PinnedAttestor> =
        if let Some(pin_path) = manifest.tool_catalog_pin_path.as_ref() {
            let pin = attest::catalog_pin::load(pin_path)?;
            tracing::info!(
                pin_signer = %pin.signer,
                pin_tool_count = pin.tools.len(),
                pin_catalog_sha = %hex::encode(pin.catalog_sha256()),
                "MCP tool-catalog pin loaded"
            );
            Some(attest::PinnedAttestor::new(pin))
        } else {
            tracing::warn!(
                "no tool_catalog_pin_path in manifest — running without \
                 catalog-poisoning defense (arXiv 2604.20994). Set \
                 `tool_catalog_pin_path` to enable."
            );
            None
        };
    // Attestor parked here for the rmcp-side wiring follow-up. Touch
    // it in a debug log so the binary doesn't hold a dead reference.
    if let Some(ref a) = tool_attestor {
        tracing::debug!(
            allow_removed_drift = manifest.allow_removed_drift,
            attestor_baseline_tools = a.baseline().tools.len(),
            "tool-catalog attestor ready"
        );
    }

    // v0.4.2 (A1) — load the optional `[role_filter]` block. Same
    // park-and-log pattern as the catalog pin: validating + building
    // the `ManifestRoleFilter` here means a malformed manifest refuses
    // startup rather than silently accepting an unenforceable filter,
    // even before per-tool dispatch wiring lands.
    let role_filter = manifest.role_filter.as_ref().map(|cfg| {
        let filter = mnemo_mcp::role_filter::ManifestRoleFilter::new(cfg.clone());
        tracing::info!(
            default_policy = ?cfg.default,
            caller_role_count = cfg.caller_roles.len(),
            allow_entries = cfg.allow.len(),
            deny_entries = cfg.deny.len(),
            is_noop = filter.is_noop(),
            "MCP role filter loaded (manifest [role_filter])"
        );
        Arc::new(filter)
    });
    if role_filter.is_none() {
        tracing::info!(
            "no [role_filter] block in manifest — every advertised tool is reachable \
             (pre-v0.4.2 behaviour preserved). See \
             https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization"
        );
    }

    // Allocate the lease store. The MCP tools layer does not consume it
    // yet — wiring `forget_subject` / `export_audit_log` to require a
    // lease scope is tracked separately. The store is exercised by the
    // unit tests in `lease.rs` and held here so the privileged path is
    // ready for that follow-up without another binary change.
    let lease_store = Arc::new(lease::LeaseStore::new(manifest.lease_ttl_seconds));
    // Periodically purge expired leases so the map cannot grow without
    // bound under repeated recall traffic.
    let purge_store = lease_store.clone();
    let purge_ttl = manifest.lease_ttl_seconds;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(purge_ttl));
        interval.tick().await;
        loop {
            interval.tick().await;
            purge_store.purge_expired();
        }
    });

    // Embedding provider: mirror the default startup path. ONNX > OpenAI > Noop.
    let embedding: Arc<dyn EmbeddingProvider> = if let Some(ref onnx_path) = cli.onnx_model_path {
        tracing::info!("Using ONNX local embeddings from {}", onnx_path);
        Arc::new(mnemo_core::embedding::onnx::OnnxEmbedding::new(
            onnx_path,
            cli.dimensions,
        )?)
    } else if let Some(ref api_key) = cli.openai_api_key {
        tracing::info!("Using OpenAI embeddings ({})", cli.embedding_model);
        Arc::new(OpenAiEmbedding::new(
            api_key.clone(),
            cli.embedding_model.clone(),
            cli.dimensions,
        ))
    } else {
        tracing::warn!(
            "No OPENAI_API_KEY set, using noop embeddings (semantic search will not work)"
        );
        Arc::new(NoopEmbedding::new(cli.dimensions))
    };

    // Storage: hardened mode is DuckDB-only (PostgreSQL connection
    // strings are exactly the kind of capability the manifest is meant
    // to keep out of env). `cli.db_path` still applies — it is the
    // path-only knob in the CLI.
    let storage = Arc::new(DuckDbStorage::open(&cli.db_path)?);
    let index = Arc::new(UsearchIndex::new(cli.dimensions)?);
    let index_path = cli.db_path.with_extension("usearch");
    if index_path.exists() {
        index.load(&index_path)?;
        tracing::info!("Loaded vector index ({} vectors)", index.len());
    }
    let ft_path = cli.db_path.with_extension("tantivy");
    let full_text = Arc::new(TantivyFullTextIndex::new(&ft_path)?);
    tracing::info!(
        "Full-text index ready at {:?} ({} docs)",
        ft_path,
        full_text.len()
    );
    let mut eng = MnemoEngine::new(
        storage,
        index.clone(),
        embedding,
        cli.agent_id.clone(),
        cli.org_id.clone(),
    )
    .with_full_text(full_text)
    .with_provenance_signer(Arc::new(signer));
    if let Some(ref key_hex) = cli.encryption_key {
        let enc = ContentEncryption::from_hex(key_hex)?;
        eng = eng.with_encryption(Arc::new(enc));
        tracing::info!("At-rest encryption enabled");
    }
    let engine = Arc::new(eng);

    let shutdown_notify = Arc::new(Notify::new());
    let signal_shutdown = shutdown_notify.clone();
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to listen for Ctrl+C: {e}");
            return;
        }
        tracing::info!("Received shutdown signal");
        signal_shutdown.notify_one();
    });

    let server = MnemoServer::new(engine);
    tracing::info!("Starting Mnemo MCP server on stdio (hardened mode)");
    let service = server.serve(stdio()).await?;
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
    let index_path = cli.db_path.with_extension("usearch");
    tracing::info!("Saving vector index ({} vectors)...", index.len());
    if let Err(e) = index.save(&index_path) {
        tracing::error!("Failed to save vector index: {}", e);
    }
    Ok(())
}

/// Handle `mnemo eval` (v0.4.0-rc3 Task B6).
///
/// Replays a JSONL dataset of `{id, content, query, expected}` rows
/// against an in-memory engine and emits a per-row JSONL report
/// (latency_ms, top_k, hit). Used to compare config sweeps
/// (provenance on/off, hybrid weights, recency half-life) against a
/// fixed prompt set without spinning up a full deployment.
async fn run_eval(cli: &Cli, args: &EvalArgs) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{BufWriter, Write};
    use std::time::Instant;

    use mnemo_core::query::recall::RecallRequest;
    use mnemo_core::query::remember::RememberRequest;

    #[derive(serde::Deserialize)]
    struct Row {
        id: String,
        content: String,
        query: String,
        expected: String,
    }

    let dataset_path = args.dataset.clone().unwrap_or_else(|| {
        // The bundled LongMemEval_M lives in mnemo-core's bench data
        // dir relative to the workspace root. We resolve via
        // CARGO_MANIFEST_DIR for robustness across the
        // `cargo install` install path.
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        here.join("..")
            .join("mnemo-core")
            .join("benches")
            .join("data")
            .join("longmemeval_m.jsonl")
    });

    let text = std::fs::read_to_string(&dataset_path)
        .map_err(|e| format!("eval: failed to read dataset {dataset_path:?}: {e}"))?;
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Row>(l).map_err(|e| format!("eval: bad row '{l}': {e}")))
        .collect::<Result<_, _>>()?;
    if rows.is_empty() {
        return Err(format!("eval: dataset {dataset_path:?} is empty").into());
    }

    // Build engine. Eval is always in-memory so a config sweep does
    // not pollute the operator's persisted DB.
    let storage = Arc::new(DuckDbStorage::open_in_memory()?);
    let index = Arc::new(UsearchIndex::new(cli.dimensions)?);
    let embedding: Arc<dyn EmbeddingProvider> = Arc::new(NoopEmbedding::new(cli.dimensions));
    let mut eng = MnemoEngine::new(
        storage,
        index,
        embedding,
        cli.agent_id.clone(),
        cli.org_id.clone(),
    );
    if args.with_provenance {
        let key_hex = args.provenance_key_hex.as_ref().ok_or(
            "eval: --with-provenance requires --provenance-key-hex (>=32 raw bytes hex-encoded)",
        )?;
        let key_bytes = hex::decode(key_hex)
            .map_err(|e| format!("eval: --provenance-key-hex not valid hex: {e}"))?;
        if key_bytes.len() < 32 {
            return Err(format!(
                "eval: --provenance-key-hex must decode to >= 32 bytes (got {})",
                key_bytes.len()
            )
            .into());
        }
        let signer = mnemo_core::provenance::ProvenanceSigner::new("eval-key", &key_bytes);
        eng = eng.with_provenance_signer(Arc::new(signer));
    }
    let engine = Arc::new(eng);

    // Seed the engine with each row's content. Eval queries hit the
    // same engine so we can measure end-to-end recall latency.
    for r in &rows {
        let mut req = RememberRequest::new(r.content.clone());
        req.tags = Some(vec![format!("eval-id:{}", r.id)]);
        engine.remember(req).await?;
    }

    // Open the output sink. None means stdout.
    let mut out: Box<dyn Write> = match &args.output {
        Some(path) => Box::new(BufWriter::new(std::fs::File::create(path)?)),
        None => Box::new(BufWriter::new(std::io::stdout().lock())),
    };

    let mut hits = 0usize;
    let mut total_latency_us: u128 = 0;
    for r in &rows {
        let recall = RecallRequest {
            query: r.query.clone(),
            agent_id: None,
            limit: Some(args.limit),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: Some(args.strategy.clone()),
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
            explain: None,
            with_provenance: if args.with_provenance {
                Some(true)
            } else {
                None
            },
        };
        let t0 = Instant::now();
        let resp = engine.recall(recall).await?;
        let elapsed_us = t0.elapsed().as_micros();
        total_latency_us += elapsed_us;

        let recalled_contents: Vec<String> =
            resp.memories.iter().map(|m| m.content.clone()).collect();
        let hit = recalled_contents
            .iter()
            .any(|c| c.to_lowercase().contains(&r.expected.to_lowercase()));
        if hit {
            hits += 1;
        }

        let row = serde_json::json!({
            "id": r.id,
            "query": r.query,
            "expected": r.expected,
            "recalled_count": resp.memories.len(),
            "recalled_top1": resp.memories.first().map(|m| m.content.clone()),
            "hit": hit,
            "latency_us": elapsed_us,
            "provenance_present": resp.provenance.is_some(),
        });
        writeln!(out, "{}", serde_json::to_string(&row)?)?;
    }

    let n = rows.len() as f64;
    let avg_latency_us = total_latency_us as f64 / n;
    let summary = serde_json::json!({
        "summary": true,
        "rows": rows.len(),
        "hits": hits,
        "hit_rate": hits as f64 / n,
        "avg_latency_us": avg_latency_us,
        "strategy": args.strategy,
        "with_provenance": args.with_provenance,
    });
    writeln!(out, "{}", serde_json::to_string(&summary)?)?;
    out.flush()?;
    Ok(())
}
