use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use clap::{Parser, Subcommand};
use rmcp::{ServiceExt, transport::stdio};
use tokio::sync::Notify;

mod attest;
mod commands;
#[cfg(feature = "http-transport")]
mod http_transport;
mod manifest;
mod safe_spawn;

use attest::CatalogAttestor;

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

    /// HMAC key (hex) for verifying per-request capabilities (ADR 0002).
    ///
    /// When set, a request may carry a signed capability — in `_meta` under
    /// `dev.mnemo/capability` on any transport, or as an
    /// `Authorization: Bearer <base64url>` header over HTTP — and its principal
    /// becomes the caller identity for that one call. Requests without a
    /// capability keep the boot-derived `--agent-id`, so setting this does not
    /// break existing clients. Presenting a capability WITHOUT this key set is
    /// an error, never a silent downgrade.
    #[arg(long, env = "MNEMO_CAPABILITY_KEY")]
    capability_key: Option<String>,

    /// Key id recorded in issued capabilities, so a rotated key can still
    /// verify tokens minted under the previous one.
    #[arg(long, default_value = "default", env = "MNEMO_CAPABILITY_KEY_ID")]
    capability_key_id: String,

    /// Serve MCP over authenticated Streamable HTTP on this port instead of stdio.
    ///
    /// Requires the `http-transport` build feature. This is the multi-caller
    /// transport #126's capability leases are designed for: each request
    /// carries its own credential, so distinct callers hold distinct
    /// identities. Requires `--capability-key`; without one the server could
    /// not tell its callers apart, which on a network-facing port means every
    /// client would silently act as the operator.
    #[arg(long, env = "MNEMO_HTTP_PORT")]
    http_port: Option<u16>,

    /// Enable capability-leased reads with this TTL in seconds (0 = disabled).
    ///
    /// When set, `mnemo.recall` returns a short-lived `lease` bound to the
    /// calling principal and `mnemo.forget_subject` REQUIRES one — binding the
    /// erasure to a read the same caller just performed, which breaks the
    /// OX-MCP exfiltrate-then-act chain (#126).
    ///
    /// This changes the contract of two shipped tools, so it is off by default.
    /// Only meaningful with `--capability-key`: without per-request identity
    /// every caller shares the boot agent id, and a lease bound to an identity
    /// everyone holds proves nothing.
    ///
    /// The lease also covers only the subjects the recall actually returned
    /// (#160), so an erasure of a subject the caller never read is refused even
    /// inside the TTL.
    #[arg(long, default_value = "0", env = "MNEMO_LEASE_TTL_SECONDS")]
    lease_ttl_seconds: u64,

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
    /// Run a measurement-only benchmark (v0.4.9).
    ///
    /// Currently exposes one subcommand: `embeddings`, which runs the
    /// embedding-backend selection bench plus an SLA-aware recommender
    /// anchored on arXiv:2605.23618 (GE2 vs local encoders — quality
    /// and latency). The bench is measurement and recommendation
    /// only: no retrieval defaults change, no RRF-weights change,
    /// no managed-cloud default.
    #[command(subcommand)]
    Bench(BenchCommand),
    /// Compliance primitives (mnemo-compliance).
    ///
    /// Currently exposes `retention`, which prints a processing-log
    /// retention-conformance profile (DPDP Rules 2025 / EU AI Act Art.19 /
    /// HIPAA §164.312(b)) and checks that the active storage backend can
    /// honour its append-only retention floor — failing loud with a typed
    /// error if it cannot.
    #[command(subcommand)]
    Compliance(ComplianceCommand),
    /// Mint per-request capabilities (ADR 0002).
    ///
    /// Verification without a way to issue tokens is an unusable feature, so
    /// this is the operator-facing half of `--capability-key`.
    #[command(subcommand)]
    Capability(CapabilityCommand),
    /// Export the memory-write hash chain for offline verification.
    ///
    /// The repository promises "a SHA-256 hash-chained log an auditor can
    /// verify offline, without trusting the store or the vendor". Every
    /// verifier that shipped before this required linking `mnemo-core` or
    /// calling a running `mnemo` — which satisfies "without trusting the
    /// store" but not "without trusting the vendor", and there was no way to
    /// get the log out of the database at all.
    ///
    /// The export is deliberately dumb: plain JSONL, primitive fields, hex
    /// hashes, no mnemo types. `tools/verify_mnemo_chain.py` reads it with
    /// nothing but the Python standard library.
    #[command(subcommand)]
    Audit(AuditCommand),
}

#[derive(Subcommand)]
enum AuditCommand {
    /// Write the chain to a JSONL file (or stdout) in verification order.
    Export(AuditExportArgs),
}

#[derive(clap::Args)]
struct AuditExportArgs {
    /// Agent whose write chain to export.
    #[arg(long, default_value = "default")]
    agent_id: String,
    /// Only records created at or after this RFC3339 timestamp.
    #[arg(long)]
    since: Option<String>,
    /// Maximum records to export.
    #[arg(long, default_value_t = 100_000)]
    limit: usize,
    /// Where to write. Defaults to stdout so the export can be piped straight
    /// into the verifier.
    #[arg(long)]
    out: Option<std::path::PathBuf>,
}

#[derive(Subcommand)]
enum CapabilityCommand {
    /// Issue a signed capability and print it.
    ///
    /// The `--format bearer` output goes straight into an
    /// `Authorization` header on the HTTP transport; `--format json` is the
    /// value to place in a request's `_meta["dev.mnemo/capability"]` on any
    /// transport, stdio included.
    Issue(CapabilityIssueArgs),
}

#[derive(clap::Args)]
struct CapabilityIssueArgs {
    /// HMAC key (hex) — must match the server's `--capability-key`.
    #[arg(long, env = "MNEMO_CAPABILITY_KEY")]
    key: String,
    /// Key id recorded in the token, so a rotated key can still verify it.
    #[arg(long, default_value = "default", env = "MNEMO_CAPABILITY_KEY_ID")]
    key_id: String,
    /// Principal this capability authenticates — becomes the caller identity.
    #[arg(long)]
    principal: String,
    /// Space-separated scope tokens. `role:<id>` entries become RBAC roles for
    /// the tool filter; everything else is an opaque scope (#126 leases).
    #[arg(long, default_value = "")]
    scope: String,
    /// Lifetime in seconds. Omit for a token that never expires — prefer a
    /// short TTL: expiry is the only revocation this token has.
    #[arg(long)]
    ttl_seconds: Option<i64>,
    /// `bearer` for an HTTP `Authorization` value, `json` for `_meta`.
    #[arg(long, value_parser = ["bearer", "json"], default_value = "bearer")]
    format: String,
}

#[derive(Subcommand)]
enum ComplianceCommand {
    /// Print a retention-conformance profile and gate it against the active
    /// storage backend's append-only guarantee.
    Retention(RetentionArgs),
}

#[derive(clap::Args)]
struct RetentionArgs {
    /// Which obligation profile to load.
    #[arg(long, value_parser = ["dpdp", "eu-ai-act-art19", "hipaa"], default_value = "dpdp")]
    profile: String,
    /// Override the retention floor in days (defaults to the obligation's
    /// legal minimum). An operator may set a *longer* floor for its policy.
    #[arg(long)]
    floor_days: Option<u32>,
}

#[derive(Subcommand)]
enum BenchCommand {
    /// Embedding-backend selection bench. For every configured backend
    /// (Noop + bench-local hashing baseline always; OpenAI if
    /// `OPENAI_API_KEY` is set; ONNX if `MNEMO_ONNX_MODEL_PATH` is set
    /// and `mnemo-core` was built with the `onnx` feature), measure
    /// nDCG@10, recall@10, p50/p95 single-vector embed latency, and
    /// throughput at batch sizes 1/8/32, then recommend the
    /// highest-nDCG backend whose p95 ≤ the SLO.
    Embeddings(BenchEmbeddingsArgs),
}

#[derive(clap::Args)]
struct BenchEmbeddingsArgs {
    /// p95 latency SLO in milliseconds. The recommender picks the
    /// highest-nDCG backend whose measured p95 ≤ this value.
    #[arg(long, default_value_t = 50.0)]
    slo_ms: f64,
    /// Embedding dimensions to construct each backend with. The
    /// fixture is small, so the default 384 is fine for both
    /// MiniLM-class local models and `text-embedding-3-small` at
    /// reduced dim.
    #[arg(long, default_value_t = 384)]
    dimensions: usize,
    /// Number of single-vector embed calls timed per backend for
    /// p50/p95. Default 32; raise for tighter percentile estimates.
    #[arg(long, default_value_t = 32)]
    latency_samples: usize,
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
    /// Print a ready-to-paste `[tool_catalog_pin]` block for the tools this
    /// exact binary advertises (after applying any manifest `[role_filter]`),
    /// then exit WITHOUT serving. Pipe into a file and set
    /// `tool_catalog_pin_path` in the manifest to enable serve-time
    /// tool-catalog attestation (arXiv 2604.20994).
    #[arg(long)]
    print_catalog_pin: bool,
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

/// DocTrace experience-memory tier gate. Off unless
/// `MNEMO_EXPERIENCE_MEMORY` is `1` / `true` (case-insensitive), so the
/// default server behaviour is unchanged.
fn experience_memory_enabled() -> bool {
    std::env::var("MNEMO_EXPERIENCE_MEMORY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
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
        Some(Command::Bench(sub)) => return run_bench(sub).await,
        Some(Command::Compliance(sub)) => return run_compliance(sub).await,
        Some(Command::Capability(sub)) => return run_capability(sub),
        Some(Command::Audit(sub)) => return run_audit(sub, &cli.db_path).await,
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
            // Share the storage pool so pgvector ANN search (semantic / auto /
            // graph / domain_scoped recall) runs against the HNSW index (#99).
            let pg_index = Arc::new(mnemo_postgres::PgVectorIndex::with_pool(
                pg_storage.pool(),
                cli.dimensions,
            ));
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
            if experience_memory_enabled() {
                eng = eng.with_experience_memory();
                tracing::info!("Experience-memory tier (DocTrace) enabled");
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
        if experience_memory_enabled() {
            eng = eng.with_experience_memory();
            tracing::info!("Experience-memory tier (DocTrace) enabled");
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

    // Per-request identity (ADR 0002). Optional: without a key, a request that
    // presents no capability keeps the boot-derived agent id exactly as before,
    // and a request that DOES present one is rejected rather than downgraded.
    let has_capability_key = cli.capability_key.is_some();
    if let Some(ref key_hex) = cli.capability_key {
        let key = hex::decode(key_hex.trim()).map_err(|e| -> Box<dyn std::error::Error> {
            format!("--capability-key must be hex-encoded HMAC key material: {e}").into()
        })?;
        server = server.with_capability_issuer(std::sync::Arc::new(
            mnemo_core::model::capability::CapabilityIssuer::new(
                cli.capability_key_id.clone(),
                &key,
            ),
        ));
        tracing::info!(
            key_id = %cli.capability_key_id,
            "Per-request capability verification enabled (ADR 0002)"
        );
    }

    // #126 — capability-leased reads. Off unless the operator sets a TTL,
    // because it changes the contract of two shipped tools.
    if cli.lease_ttl_seconds > 0 {
        if !has_capability_key {
            return Err(
                "--lease-ttl-seconds requires --capability-key (or MNEMO_CAPABILITY_KEY).\n\
                 \n\
                 A lease binds an erasure to a read by the SAME caller. Without per-request \
                 identity every caller resolves to the boot --agent-id, so every lease would \
                 validate for every caller — ceremony that looks like a defence and is not."
                    .into(),
            );
        }
        let store = std::sync::Arc::new(mnemo_mcp::lease::LeaseStore::new(cli.lease_ttl_seconds));
        // Bound memory on a long-lived server. `check` already refuses expired
        // leases, so this is hygiene rather than enforcement.
        let purge_handle = store.clone();
        let purge_every = std::time::Duration::from_secs(cli.lease_ttl_seconds.max(1));
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(purge_every);
            loop {
                ticker.tick().await;
                purge_handle.purge_expired();
            }
        });
        server = server.with_lease_store(store);
        tracing::info!(
            ttl_seconds = cli.lease_ttl_seconds,
            "Capability-leased reads enabled (#126): mnemo.forget_subject now requires a lease \
             from mnemo.recall"
        );
    }

    // The HTTP transport replaces stdio rather than running alongside it: one
    // process serves one MCP transport, and a server reachable both ways would
    // have two identity paths into the same engine.
    #[cfg(feature = "http-transport")]
    if let Some(http_port) = cli.http_port {
        return http_transport::serve(server, http_port, has_capability_key).await;
    }
    #[cfg(not(feature = "http-transport"))]
    if cli.http_port.is_some() {
        return Err(
            "--http-port needs the `http-transport` build feature; this binary was built without \
             it. Rebuild with `--features http-transport`, or omit --http-port to serve on stdio."
                .into(),
        );
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
/// existing MCP STDIO server.
async fn run_mcp_server(cli: &Cli, args: &McpServerArgs) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = manifest::Manifest::load(&args.manifest)?;
    tracing::info!(
        manifest = ?args.manifest,
        allowed_tools = ?manifest.allowed_tools,
        allowed_parents = ?manifest.allowed_parents,
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

    // v0.4.0 (P0-1) — load the optional tool-catalog pin and build an attestor.
    // A malformed pin refuses startup here; the actual attestation against the
    // server's advertised catalog runs once below, after the server (and any
    // role filter) is built and before it serves stdio.
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
    // v0.4.2 (A1) — load the optional `[role_filter]` block. Building the
    // `ManifestRoleFilter` here means a malformed manifest refuses startup rather
    // than silently accepting an unenforceable filter, and the filter is attached to
    // the server below, so a denied tool is hidden from `tools/list` and rejected by
    // `tools/call` with `-32601`.
    //
    // Transport limitation (the thing an auditor asks about): the stdio transport
    // carries no per-call caller identity, so dispatch builds a `CallerContext` from
    // the engine's default agent id with no roles. A `deny` list therefore acts as a
    // server-wide tool denylist on the stdio transport, not as per-caller RBAC.
    let role_filter = manifest.role_filter.as_ref().map(|cfg| {
        let filter = mnemo_mcp::role_filter::ManifestRoleFilter::new(cfg.clone());
        if filter.is_noop() {
            // A `[role_filter]` block that denies nothing is a real footgun: the
            // operator likely intended a restriction that is not expressed here.
            tracing::warn!(
                default_policy = ?cfg.default,
                caller_role_count = cfg.caller_roles.len(),
                allow_entries = cfg.allow.len(),
                deny_entries = cfg.deny.len(),
                is_noop = true,
                "MCP [role_filter] block present but is a no-op (no roles, no allow, no \
                 deny, default allow_all) — every tool stays reachable; it enforces nothing"
            );
        } else {
            tracing::info!(
                default_policy = ?cfg.default,
                caller_role_count = cfg.caller_roles.len(),
                allow_entries = cfg.allow.len(),
                deny_entries = cfg.deny.len(),
                is_noop = false,
                "MCP [role_filter] enforcement active — a denied tool is hidden from \
                 tools/list and rejected by tools/call with -32601. Note: on stdio there \
                 is no per-call caller identity, so this is a server-wide tool denylist, \
                 not per-caller RBAC"
            );
        }
        Arc::new(filter)
    });
    if role_filter.is_none() {
        tracing::info!(
            "no [role_filter] block in manifest — every advertised tool is reachable \
             (pre-v0.4.2 behaviour preserved). See \
             https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization"
        );
    }

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

    let mut server = MnemoServer::new(engine.clone());
    if let Some(filter) = role_filter.clone() {
        server = server.with_role_filter(filter);
    }

    // `--print-catalog-pin`: emit a ready-to-paste pin for exactly the tools
    // this binary advertises (post role-filter), then exit WITHOUT serving.
    if args.print_catalog_pin {
        let toml = render_catalog_pin_toml(
            &server.advertised_tool_catalog(),
            "REPLACE-ME:catalog-pin",
            &chrono::Utc::now().to_rfc3339(),
        );
        print!("{toml}");
        return Ok(());
    }

    // v0.4.0 (P0-1) — serve-time tool-catalog attestation (arXiv 2604.20994).
    // Runs ONCE, over the compiled `#[tool]` catalog the server will advertise
    // (post role-filter), before it serves stdio. On stdio the catalog is static
    // after boot, so a boot-time check is complete for this transport: it defends
    // against a substituted/tampered binary, a hostile dependency that injects or
    // renames a tool, and pin drift after a version bump. It is NOT per-request
    // and is only as strong as the manifest file's permissions.
    if let Some(attestor) = tool_attestor.as_ref() {
        let fingerprints: Vec<attest::ToolFingerprint> = server
            .advertised_tool_catalog()
            .iter()
            .map(|(name, desc, schema)| attest::fingerprint_tool(name, desc, schema))
            .collect();
        let verdict = attestor.attest(&fingerprints)?;
        // Audit EVERY verdict (before any early Err) so even a refused startup
        // leaves an `mcp_tool_catalog_drift` trail. The module doc promises this.
        record_catalog_drift_event(&engine, &cli.agent_id, &verdict).await;

        let removed_only = verdict.is_removed_only_drift();
        match &verdict {
            attest::AttestationVerdict::Match => {
                tracing::info!(
                    pin_signer = %attestor.baseline().signer,
                    catalog_sha = %hex::encode(attestor.baseline().catalog_sha256()),
                    tool_count = fingerprints.len(),
                    "MCP tool-catalog attestation PASSED — advertised catalog matches the pin"
                );
            }
            attest::AttestationVerdict::Drift { removed, .. }
                if removed_only && manifest.allow_removed_drift =>
            {
                let names: Vec<&str> = removed.iter().map(|t| t.name.as_str()).collect();
                tracing::warn!(
                    removed = ?names,
                    "MCP tool-catalog removed-only drift ACCEPTED via allow_removed_drift — \
                     the advertised catalog is a subset of the pin"
                );
            }
            attest::AttestationVerdict::Drift {
                added,
                removed,
                mutated,
            } => {
                let names = |v: &[attest::ToolFingerprint]| {
                    v.iter().map(|t| t.name.clone()).collect::<Vec<_>>()
                };
                return Err(format!(
                    "MCP tool-catalog attestation FAILED (arXiv 2604.20994): advertised catalog \
                     drifted from the pin — added={:?} mutated={:?} removed={:?}. Refusing to \
                     serve. Regenerate the pin with `--print-catalog-pin` if this change is \
                     intended{}.",
                    names(added.as_slice()),
                    names(mutated.as_slice()),
                    names(removed.as_slice()),
                    if removed_only {
                        ", or set allow_removed_drift = true for a removed-only downgrade"
                    } else {
                        ""
                    }
                )
                .into());
            }
            attest::AttestationVerdict::Reject { reason } => {
                return Err(format!(
                    "MCP tool-catalog attestation REJECTED (arXiv 2604.20994): {reason}. \
                     Refusing to serve."
                )
                .into());
            }
        }
    }

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

/// Wrap a value as a TOML basic string, escaping `\` and `"`. Our values
/// (tool names, an RFC3339 timestamp, a signer id) carry no control chars.
fn toml_basic_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Render a ready-to-paste `[tool_catalog_pin]` block from an advertised
/// catalog (`(name, description, input_schema_json)` triples). The emitted TOML
/// round-trips through [`attest::catalog_pin::load`] (asserted in tests), and
/// each `schema_sha256` is the same fingerprint the attestor computes, so a pin
/// generated from a binary Matches that binary's own advertised catalog.
fn render_catalog_pin_toml(
    catalog: &[(String, String, String)],
    signer: &str,
    signed_at: &str,
) -> String {
    let mut out = String::new();
    out.push_str(
        "# Generated by `mnemo mcp-server --manifest <m> --print-catalog-pin`.\n\
         # Point the manifest's `tool_catalog_pin_path` at this file to enable\n\
         # serve-time tool-catalog attestation (arXiv 2604.20994). Replace the\n\
         # signer placeholder with a stable `<host>:<key_id>` before committing.\n",
    );
    out.push_str("[tool_catalog_pin]\n");
    out.push_str(&format!("signer = {}\n", toml_basic_string(signer)));
    out.push_str(&format!("signed_at = {}\n", toml_basic_string(signed_at)));
    for (name, desc, schema) in catalog {
        let fp = attest::fingerprint_tool(name, desc, schema);
        out.push_str("\n[[tool_catalog_pin.tools]]\n");
        out.push_str(&format!("name = {}\n", toml_basic_string(name)));
        out.push_str(&format!("schema_sha256 = \"{}\"\n", fp.schema_hex()));
    }
    out
}

/// Append one `McpToolCatalogDrift` audit event recording an attestation
/// verdict (the module doc promises "Every verdict is recorded"). Best-effort:
/// a storage error is logged, never propagated, so it can run before a refused
/// startup returns `Err` without masking the real reason.
async fn record_catalog_drift_event(
    engine: &mnemo_core::query::MnemoEngine,
    agent_id: &str,
    verdict: &attest::AttestationVerdict,
) {
    let names =
        |v: &[attest::ToolFingerprint]| v.iter().map(|t| t.name.clone()).collect::<Vec<String>>();
    let (label, added, removed, mutated, reason) = match verdict {
        attest::AttestationVerdict::Match => ("match", vec![], vec![], vec![], None),
        attest::AttestationVerdict::Drift {
            added,
            removed,
            mutated,
        } => ("drift", names(added), names(removed), names(mutated), None),
        attest::AttestationVerdict::Reject { reason } => {
            ("reject", vec![], vec![], vec![], Some(reason.clone()))
        }
    };
    let payload = serde_json::json!({
        "verdict": label,
        "added": added,
        "removed": removed,
        "mutated": mutated,
        "reason": reason,
    });
    let event = mnemo_core::query::event_builder::build_event(
        engine,
        agent_id,
        mnemo_core::model::event::EventType::McpToolCatalogDrift,
        payload,
        "mcp_tool_catalog_attestation",
        None,
    )
    .await;
    if let Err(e) = engine.storage.append_event_chained(&event).await {
        tracing::error!(event_id = %event.id, error = %e, "failed to record McpToolCatalogDrift audit event");
    }
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
            mode: None,
            current_fact_resolver: None,
            orientation_cache: None,
            evidence_budget: None,
            retained_token_budget: None,
            domain_scope: None,
            reasoning_trust: None,
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

/// v0.4.9 — `mnemo bench <subcommand>` dispatch.
async fn run_bench(sub: &BenchCommand) -> Result<(), Box<dyn std::error::Error>> {
    match sub {
        BenchCommand::Embeddings(args) => run_bench_embeddings(args).await,
    }
}

/// Mint a capability (ADR 0002). Synchronous: signing touches no I/O.
fn run_capability(sub: &CapabilityCommand) -> Result<(), Box<dyn std::error::Error>> {
    let CapabilityCommand::Issue(args) = sub;

    let key = hex::decode(args.key.trim()).map_err(|e| -> Box<dyn std::error::Error> {
        format!("--key must be hex-encoded HMAC key material: {e}").into()
    })?;
    let issuer = mnemo_core::model::capability::CapabilityIssuer::new(args.key_id.clone(), &key);
    let capability = issuer.issue(
        args.principal.clone(),
        args.scope.clone(),
        args.ttl_seconds.map(chrono::Duration::seconds),
    );

    match args.format.as_str() {
        // Encoding lives in mnemo-mcp so the minting side cannot drift from the
        // server's decoder — one function, both directions.
        "bearer" => println!(
            "{}",
            mnemo_mcp::identity::bearer_from_capability(&capability)
        ),
        _ => println!("{}", serde_json::to_string_pretty(&capability)?),
    }
    Ok(())
}

/// Reorder only the records that share a `created_at`, by following the chain.
///
/// See the long comment at the call site for why this is necessary and why it
/// cannot turn a tampered log into a passing one. `unresolved` counts records in
/// a tie group that could not be linked — a real signal, surfaced rather than
/// smoothed over.
/// Put `records` in **chain** order: head first, then each record that links to
/// the one before it.
///
/// # Why timestamp order is not chain order
///
/// `list_memories_by_agent_ordered` sorts `created_at ASC`, and `created_at`
/// records when a write *started*. Under concurrency that is not the order the
/// writes were inserted in, and the chain is built against what was inserted.
/// A write that begins earlier and lands later leaves the two orders permanently
/// out of step — not just inside a tie group, which is all the previous version
/// of this function repaired. `verify_chain` defines each link against the
/// PRECEDING record, so exporting in timestamp order reports a break that is an
/// artefact of the sort rather than evidence of tampering, and an auditor cannot
/// tell the two apart. (Sorting by `id` does not help: the ids are UUID-v7 and
/// within a millisecond the random tail dominates, giving a third order that is
/// also not write order.)
///
/// # This cannot launder a tampered log
///
/// * An edited `content` leaves every hash untouched, so the ordering is
///   unaffected and the verifier still catches the content mismatch.
/// * A removed record leaves the records after the gap unreachable from the
///   head. They are counted in `unresolved`, reported to stderr, and emitted in
///   storage order — where the verifier still fails on them.
/// * A wholly re-forged chain is not detectable by any hash check. That needs a
///   signature, which is a different artefact with a different key.
///
/// # Cost
///
/// The fast path is a single linear pass: if the records already satisfy what
/// `verify_chain` checks — which is the case for any log written serially, and
/// for any `--since` slice of one — they are returned untouched. Only a log that
/// is genuinely out of order pays for the walk, which is quadratic in the number
/// of records because the link `SHA256(content_hash ‖ predecessor_content_hash)`
/// cannot be inverted to index a predecessor directly.
fn order_by_chain(
    records: Vec<mnemo_core::model::memory::MemoryRecord>,
    unresolved: &mut usize,
) -> Vec<mnemo_core::model::memory::MemoryRecord> {
    use mnemo_core::hash::compute_chain_hash;

    let links_to = |r: &mnemo_core::model::memory::MemoryRecord, prev: Option<&[u8]>| -> bool {
        r.prev_hash.as_deref() == Some(compute_chain_hash(&r.content_hash, prev).as_slice())
    };

    // Fast path: the records already satisfy what `verify_chain` checks.
    //
    // The first record's link is deliberately not checked, exactly as
    // `verify_chain` does not check it. `--since` hands us a slice whose first
    // record links to something that was filtered out, and that slice is a
    // perfectly good export — declaring it broken would make the flag useless.
    let mut prev: Option<&[u8]> = None;
    let already = records.iter().enumerate().all(|(i, r)| {
        let ok = i == 0 || links_to(r, prev);
        prev = Some(&r.content_hash);
        ok
    });
    if already {
        return records;
    }

    // Slow path. Start from the record nothing else follows on from: the head of
    // whatever chain or slice this is. `links_to(r, None)` finds a true chain
    // head; a `--since` slice has no such record, so fall back to the one that
    // is not any other record's successor.
    let successor_of = |s: &mnemo_core::model::memory::MemoryRecord| -> bool {
        records
            .iter()
            .any(|r| !std::ptr::eq(r, s) && links_to(s, Some(&r.content_hash)))
    };
    let start = records
        .iter()
        .position(|r| links_to(r, None))
        .or_else(|| records.iter().position(|r| !successor_of(r)));

    let mut used = vec![false; records.len()];
    let mut out = Vec::with_capacity(records.len());
    let mut prev_ch: Option<Vec<u8>> = match start {
        Some(i) => {
            used[i] = true;
            out.push(records[i].clone());
            Some(records[i].content_hash.clone())
        }
        // Every record follows some other record: a cycle, or a set with no
        // beginning. Neither is a chain, and inventing a start would hide it.
        None => {
            *unresolved += records.len();
            return records;
        }
    };
    for _ in 1..records.len() {
        let found = records
            .iter()
            .enumerate()
            .position(|(i, r)| !used[i] && links_to(r, prev_ch.as_deref()));
        match found {
            Some(i) => {
                used[i] = true;
                prev_ch = Some(records[i].content_hash.clone());
                out.push(records[i].clone());
            }
            // Nothing links here: the chain stops. Whatever is left is either a
            // gap in the log or a second chain, and both must reach the verifier
            // rather than be tidied away.
            None => break,
        }
    }
    for (i, r) in records.iter().enumerate() {
        if !used[i] {
            *unresolved += 1;
            out.push(r.clone());
        }
    }
    out
}

/// Export the memory-write hash chain as JSONL for offline verification.
///
/// # Why the format is this boring
///
/// Every field is a JSON string or null. Hashes are lowercase hex. There is no
/// nesting, no enum tag, no schema version negotiated at runtime. The verifier
/// (`tools/verify_mnemo_chain.py`) must be readable end to end by an auditor in
/// a few minutes, and every type it has to understand is a type it has to
/// trust. If the export needed a mnemo type to parse, the format would be
/// wrong.
///
/// The record order is the order `verify_chain` expects — oldest first, by
/// creation — because the chain link is defined against the PRECEDING record.
/// Exporting in any other order would produce a file that fails verification
/// for a reason that has nothing to do with tampering.
async fn run_audit(
    sub: &AuditCommand,
    db_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let AuditCommand::Export(args) = sub;

    let storage = DuckDbStorage::open(db_path)?;
    let mut records = storage
        .list_memories_by_agent_ordered(&args.agent_id, None, args.limit)
        .await?;

    if let Some(ref since) = args.since {
        records.retain(|r| r.created_at.as_str() >= since.as_str());
    }

    // Put the records in CHAIN order, which is not the same as the storage
    // layer's `created_at ASC`. See `order_by_chain` for why, and for why this
    // cannot turn a tampered log into one that verifies.
    let mut unlinked = 0usize;
    records = order_by_chain(records, &mut unlinked);
    if unlinked > 0 {
        eprintln!(
            "warning: {unlinked} record(s) could not be reached by following the chain \
             from its head. They are emitted in storage order. This is what a gap in the \
             log looks like from here — verification will fail, and that failure is real."
        );
    }

    // Soft-deleted records stay in the chain. Dropping them would silently
    // break every link after the gap, and an auditor would be unable to tell
    // that from tampering — which is exactly the confusion this export exists
    // to remove. The flag is exported so the reader can see what was retracted.
    let mut out: Box<dyn std::io::Write> = match args.out {
        Some(ref p) => Box::new(std::io::BufWriter::new(std::fs::File::create(p)?)),
        None => Box::new(std::io::BufWriter::new(std::io::stdout())),
    };

    let n = records.len();
    for (i, r) in records.iter().enumerate() {
        let line = serde_json::json!({
            "index": i,
            "id": r.id.to_string(),
            "agent_id": r.agent_id,
            "content": r.content,
            "created_at": r.created_at,
            "content_hash": hex::encode(&r.content_hash),
            "prev_hash": r.prev_hash.as_ref().map(hex::encode),
            "deleted_at": r.deleted_at,
        });
        use std::io::Write as _;
        writeln!(out, "{line}")?;
    }
    use std::io::Write as _;
    out.flush()?;

    // Only when writing to a file: on stdout the export IS the output, and a
    // trailing note would land in the auditor's chain.jsonl.
    if let Some(path) = &args.out {
        eprintln!(
            "exported {n} record(s) for agent '{}'. Verify with:\n  \
             python3 tools/verify_mnemo_chain.py {}",
            args.agent_id,
            path.display()
        );
    }
    Ok(())
}

async fn run_compliance(sub: &ComplianceCommand) -> Result<(), Box<dyn std::error::Error>> {
    match sub {
        ComplianceCommand::Retention(args) => run_compliance_retention(args).await,
    }
}

async fn run_compliance_retention(args: &RetentionArgs) -> Result<(), Box<dyn std::error::Error>> {
    use mnemo_compliance::RetentionProfile;

    let mut profile = match args.profile.as_str() {
        "dpdp" => RetentionProfile::dpdp_rules(),
        "eu-ai-act-art19" => RetentionProfile::eu_ai_act_art19(),
        "hipaa" => RetentionProfile::hipaa_164_312b(),
        other => return Err(format!("unknown retention profile '{other}'").into()),
    };
    if let Some(days) = args.floor_days {
        profile = profile.with_floor_days(days);
    }

    // Gate the floor against the active backend's append-only guarantee. The
    // default embedded backend is DuckDB, whose `agent_events` log is
    // append-only, so this passes; a backend that could not honour the floor
    // would surface `ComplianceError::RetentionFloorUnsupported` naming itself.
    let storage = DuckDbStorage::open_in_memory()?;
    let backend = storage.backend_name();
    let backend_ok = profile
        .assert_backend_can_retain(backend, storage.events_are_append_only())
        .is_ok();

    let out = serde_json::json!({
        "profile": profile.name,
        "obligation": profile.obligation,
        "floor_days": profile.floor_days,
        "commencement": profile.commencement,
        "source_url": profile.source_url,
        "backend": backend,
        "backend_can_honour_floor": backend_ok,
    });
    println!("{}", serde_json::to_string_pretty(&out)?);

    // Fail loud (non-zero exit) if the backend cannot honour the floor.
    profile.assert_backend_can_retain(backend, storage.events_are_append_only())?;
    Ok(())
}

async fn run_bench_embeddings(
    args: &BenchEmbeddingsArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let opts = mnemo_embeddings_bench::RunOptions {
        dimensions: args.dimensions,
        latency_samples: args.latency_samples,
    };
    let results = mnemo_embeddings_bench::run_all(&opts).await;
    let rows: Vec<mnemo_embeddings_bench::BackendRow> = results
        .iter()
        .filter_map(|(_, row, _)| row.clone())
        .collect();
    let rec = mnemo_embeddings_bench::recommend(&rows, args.slo_ms);
    let table = mnemo_embeddings_bench::render_table(&results, &rec);
    print!("{table}");
    Ok(())
}

#[cfg(test)]
mod catalog_pin_tests {
    use super::*;

    /// The `--print-catalog-pin` output must round-trip through the loader, and
    /// the loaded fingerprints must equal what the attestor computes from the
    /// same catalog — i.e. a pin generated from a binary Matches that binary.
    #[test]
    fn rendered_pin_round_trips_and_matches() {
        let catalog = vec![
            (
                "mnemo.recall".to_string(),
                "Search memories".to_string(),
                r#"{"type":"object"}"#.to_string(),
            ),
            (
                "mnemo.verify".to_string(),
                "Verify hash chain".to_string(),
                r#"{"type":"object","properties":{}}"#.to_string(),
            ),
        ];
        let toml = render_catalog_pin_toml(&catalog, "test:signer", "2026-07-30T00:00:00Z");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pin.toml");
        std::fs::write(&path, &toml).unwrap();
        let pin = attest::catalog_pin::load(&path).expect("emitted pin must load");
        assert_eq!(pin.signer, "test:signer");
        assert_eq!(pin.tools.len(), 2);

        // The loaded pin must MATCH the same catalog run through the attestor.
        let fingerprints: Vec<attest::ToolFingerprint> = catalog
            .iter()
            .map(|(n, d, s)| attest::fingerprint_tool(n, d, s))
            .collect();
        let attestor = attest::PinnedAttestor::new(pin);
        assert_eq!(
            attestor.attest(&fingerprints).unwrap(),
            attest::AttestationVerdict::Match
        );
    }
}

/// The export's record ORDER is a correctness property, not a cosmetic one:
/// each chain link is defined against the preceding record, so a file emitted in
/// the wrong order fails verification for a reason that has nothing to do with
/// tampering — and an auditor cannot tell those two apart.
#[cfg(test)]
mod export_order_tests {
    use super::order_by_chain;

    /// A chain whose records are handed to the exporter in the WRONG order must
    /// come out in chain order.
    ///
    /// This is the case the previous tie-only walker could not repair: the three
    /// records have distinct timestamps, so no tie group exists to reorder, and
    /// yet the timestamp order is not the write order — which is exactly what
    /// concurrency produces, since `created_at` records when a write started and
    /// the chain records what was inserted.
    #[test]
    fn order_by_chain_repairs_an_out_of_order_export() {
        use mnemo_core::hash::{compute_chain_hash, compute_content_hash};
        use mnemo_core::model::memory::{
            ConsolidationState, MemoryRecord, MemoryType, Scope, SourceType,
        };

        let agent = "exporter";
        let mk = |content: &str, ts: &str, prev: Option<&[u8]>| {
            let ch = compute_content_hash(content, agent, ts);
            let ph = compute_chain_hash(&ch, prev);
            MemoryRecord {
                id: uuid::Uuid::now_v7(),
                agent_id: agent.to_string(),
                content: content.to_string(),
                memory_type: MemoryType::Semantic,
                scope: Scope::Private,
                importance: 0.5,
                tags: vec![],
                metadata: serde_json::Value::Null,
                embedding: None,
                content_hash: ch,
                prev_hash: Some(ph),
                source_type: SourceType::Agent,
                source_id: None,
                consolidation_state: ConsolidationState::Raw,
                access_count: 0,
                org_id: None,
                thread_id: None,
                created_at: ts.to_string(),
                updated_at: ts.to_string(),
                last_accessed_at: None,
                expires_at: None,
                deleted_at: None,
                decay_rate: None,
                created_by: None,
                version: 1,
                prev_version_id: None,
                quarantined: false,
                quarantine_reason: None,
                decay_function: None,
            }
        };

        // Written a, b, c — but `created_at` says c, a, b, which is the order the
        // storage layer will hand them over in.
        let a = mk("first written", "2026-09-02T00:00:05+00:00", None);
        let b = mk(
            "second written",
            "2026-09-02T00:00:09+00:00",
            Some(&a.content_hash),
        );
        let c = mk(
            "third written",
            "2026-09-02T00:00:01+00:00",
            Some(&b.content_hash),
        );

        let mut by_timestamp = vec![c.clone(), a.clone(), b.clone()];
        by_timestamp.sort_by(|x, y| x.created_at.cmp(&y.created_at));
        assert_eq!(
            by_timestamp[0].content, "third written",
            "the timestamp sort must actually be wrong, or this test proves nothing"
        );

        let mut unresolved = 0usize;
        let ordered = order_by_chain(by_timestamp, &mut unresolved);
        assert_eq!(unresolved, 0, "every record is reachable from the head");
        assert_eq!(
            ordered
                .iter()
                .map(|r| r.content.as_str())
                .collect::<Vec<_>>(),
            vec!["first written", "second written", "third written"],
        );
    }

    /// A record removed from the middle leaves everything after it unreachable.
    /// Those must be reported and emitted, not quietly dropped — a shorter file
    /// that verifies is the one outcome an audit export must never produce.
    #[test]
    fn order_by_chain_reports_records_it_cannot_reach() {
        use mnemo_core::hash::{compute_chain_hash, compute_content_hash};
        use mnemo_core::model::memory::{
            ConsolidationState, MemoryRecord, MemoryType, Scope, SourceType,
        };

        let agent = "exporter";
        let mk = |content: &str, ts: &str, prev: Option<&[u8]>| {
            let ch = compute_content_hash(content, agent, ts);
            let ph = compute_chain_hash(&ch, prev);
            MemoryRecord {
                id: uuid::Uuid::now_v7(),
                agent_id: agent.to_string(),
                content: content.to_string(),
                memory_type: MemoryType::Semantic,
                scope: Scope::Private,
                importance: 0.5,
                tags: vec![],
                metadata: serde_json::Value::Null,
                embedding: None,
                content_hash: ch,
                prev_hash: Some(ph),
                source_type: SourceType::Agent,
                source_id: None,
                consolidation_state: ConsolidationState::Raw,
                access_count: 0,
                org_id: None,
                thread_id: None,
                created_at: ts.to_string(),
                updated_at: ts.to_string(),
                last_accessed_at: None,
                expires_at: None,
                deleted_at: None,
                decay_rate: None,
                created_by: None,
                version: 1,
                prev_version_id: None,
                quarantined: false,
                quarantine_reason: None,
                decay_function: None,
            }
        };

        let a = mk("kept", "2026-09-02T00:00:01+00:00", None);
        let b = mk(
            "removed",
            "2026-09-02T00:00:02+00:00",
            Some(&a.content_hash),
        );
        let c = mk(
            "orphaned by the removal",
            "2026-09-02T00:00:03+00:00",
            Some(&b.content_hash),
        );

        let mut unresolved = 0usize;
        let ordered = order_by_chain(vec![a, c], &mut unresolved);
        assert_eq!(unresolved, 1, "the orphan must be counted, not dropped");
        assert_eq!(ordered.len(), 2, "and still emitted");
        assert_eq!(ordered[1].content, "orphaned by the removal");
    }

    /// `--since` hands the exporter a slice whose first record links to
    /// something that was filtered out. That slice is a perfectly good export —
    /// `verify_chain` does not check the first record's link either — and it
    /// must come back untouched rather than be declared unreachable.
    #[test]
    fn order_by_chain_leaves_a_since_slice_alone() {
        use mnemo_core::hash::{compute_chain_hash, compute_content_hash};
        use mnemo_core::model::memory::{
            ConsolidationState, MemoryRecord, MemoryType, Scope, SourceType,
        };

        let agent = "exporter";
        let mk = |content: &str, ts: &str, prev: Option<&[u8]>| {
            let ch = compute_content_hash(content, agent, ts);
            let ph = compute_chain_hash(&ch, prev);
            MemoryRecord {
                id: uuid::Uuid::now_v7(),
                agent_id: agent.to_string(),
                content: content.to_string(),
                memory_type: MemoryType::Semantic,
                scope: Scope::Private,
                importance: 0.5,
                tags: vec![],
                metadata: serde_json::Value::Null,
                embedding: None,
                content_hash: ch,
                prev_hash: Some(ph),
                source_type: SourceType::Agent,
                source_id: None,
                consolidation_state: ConsolidationState::Raw,
                access_count: 0,
                org_id: None,
                thread_id: None,
                created_at: ts.to_string(),
                updated_at: ts.to_string(),
                last_accessed_at: None,
                expires_at: None,
                deleted_at: None,
                decay_rate: None,
                created_by: None,
                version: 1,
                prev_version_id: None,
                quarantined: false,
                quarantine_reason: None,
                decay_function: None,
            }
        };

        let a = mk("before the cutoff", "2026-09-02T00:00:01+00:00", None);
        let b = mk(
            "after the cutoff",
            "2026-09-02T00:00:02+00:00",
            Some(&a.content_hash),
        );
        let c = mk(
            "also after",
            "2026-09-02T00:00:03+00:00",
            Some(&b.content_hash),
        );

        // `--since` dropped `a`.
        let mut unresolved = 0usize;
        let ordered = order_by_chain(vec![b, c], &mut unresolved);
        assert_eq!(
            unresolved, 0,
            "a --since slice is not a gap in the log and must not be reported as one"
        );
        assert_eq!(
            ordered
                .iter()
                .map(|r| r.content.as_str())
                .collect::<Vec<_>>(),
            vec!["after the cutoff", "also after"],
        );
    }
}
