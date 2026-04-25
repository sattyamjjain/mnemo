//! Deterministic generator for golden DuckDB persistence fixtures
//! (v0.4.0-rc1 Task A7 — partial; see issue #38).
//!
//! Run with:
//!
//! ```text
//! cargo run --release --bin gen-golden-fixture -- crates/mnemo-core/tests/golden/v0_3_4.mnemo.db
//! ```
//!
//! The output is a DuckDB file populated with a fixed set of records
//! using a frozen clock (`2026-04-25T00:00:00Z`) and deterministic
//! UUIDs. The hash chain is therefore byte-stable: re-running the
//! generator produces a file with the same SHA-256 modulo a few
//! `updated_at`-style fields DuckDB writes itself. The migration
//! round-trip test compares structural invariants rather than the
//! whole file digest to stay robust against those.
//!
//! WHY NO v0.1.1 / v0.3.0 FIXTURES YET
//! -----------------------------------
//!
//! The 2026-04-25 prompt called for fixtures pinned to git tags
//! `v0.1.1` and `v0.3.0`. Those tags don't exist on this repo —
//! `git tag` returns only `v0.3.3` (this session) and `v0.3.4`. The
//! historical tag work is tracked under issue #38; this generator
//! covers the v0.3.4-shape baseline so we don't ship v0.4.0 without
//! ANY round-trip coverage.
//!
//! Wire as a workspace binary in mnemo-core's Cargo.toml when
//! adopting; left as a standalone source file today so there's no
//! workspace-level rebuild cost.

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::duckdb::DuckDbStorage;

const FROZEN_RECORDS: &[(&str, &str, f32)] = &[
    ("agent-fixture", "Project kickoff on 2026-02-01.", 0.7),
    (
        "agent-fixture",
        "User prefers dark mode for the dashboard UI.",
        0.4,
    ),
    (
        "agent-fixture",
        "Quarterly revenue forecast landed at $42M.",
        0.6,
    ),
    (
        "agent-fixture",
        "DuckDB 1.5.2 release includes DuckLake v1 support.",
        0.3,
    ),
    (
        "agent-fixture",
        "MnemoMemoryToolServer ships in v0.3.4 against memory_20250818.",
        0.5,
    ),
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: gen_golden_fixtures <out.mnemo.db>");
        std::process::exit(2);
    }
    let out: PathBuf = args[1].parse().unwrap_or_else(|_| PathBuf::from(&args[1]));
    if out.exists() {
        std::fs::remove_file(&out)?;
    }

    let storage = Arc::new(DuckDbStorage::open(&out)?);
    let index = Arc::new(UsearchIndex::new(64)?);
    let embedding = Arc::new(NoopEmbedding::new(64));
    let engine = Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        "agent-fixture".to_string(),
        None,
    ));

    for (agent, content, importance) in FROZEN_RECORDS {
        let req = RememberRequest {
            content: (*content).to_string(),
            agent_id: Some((*agent).to_string()),
            memory_type: None,
            scope: None,
            importance: Some(*importance),
            tags: Some(vec!["fixture".to_string()]),
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: Some("fixture-thread".to_string()),
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        };
        engine.remember(req).await?;
    }

    println!(
        "wrote {} records to {}",
        FROZEN_RECORDS.len(),
        out.display()
    );
    Ok(())
}
