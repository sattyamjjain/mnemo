//! v0.4.1 (P0-2) — Anthropic CMA-Memory compat shim.
//!
//! Anthropic shipped Context-Managed Agent (CMA) Memory in public
//! beta on 2026-04-23 with `anthropic-sdk-python 0.97.0` +
//! `claude-agent-sdk 0.1.68`. The data model is a Markdown filesystem
//! tree at `<root>/.memory/` with a sibling `audit.jsonl` log. The
//! beta is the most direct competitive surface mnemo has seen all
//! month — operators who pick CMA for Anthropic's branding can keep
//! using the SDK while mnemo provides:
//!
//! 1. The Markdown tree on disk (read-through / write-through /
//!    mirror modes — see [`SyncMode`]).
//! 2. A bridged audit log: every CMA write produces exactly one
//!    `mnemo` `AuditEvent` whose `prev_hash` chains into the engine's
//!    HMAC ledger.
//! 3. A one-shot importer that walks an existing CMA tree, ingests
//!    every Markdown file as a Mnemo memory, and stamps the bridge
//!    head into the audit log.
//! 4. An export back to a byte-identical CMA tree so users can
//!    leave mnemo cleanly.
//!
//! Tested against `claude-agent-sdk-python==0.1.68` (pinned dev-dep
//! in the repo's Python SDK + a "tested-against" badge in the
//! README — drift-watcher CI is tracked under issue #cma-drift).

pub mod audit_bridge;
pub mod migrate;
pub mod tree;

pub use audit_bridge::{BridgeError, BridgedEvent, CmaSource, bridge_event};
pub use migrate::{ImportSummary, export_to_tree, import_cma_tree};
pub use tree::{CmaTreeRoot, SyncMode};
