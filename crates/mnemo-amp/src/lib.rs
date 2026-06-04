//! # mnemo-amp
//!
//! AMP / *memorywire* interop adapter for [mnemo](https://github.com/sattyamjjain/mnemo).
//!
//! AMP models an agent's memory surface as **5 operations**
//! (`remember` / `recall` / `forget` / `merge` / `expire`) over **4
//! memory types** (`episodic` / `semantic` / `procedural` /
//! `working`), carried in a self-describing JSON envelope validated
//! against a JSON-Schema 2020-12 document. This crate implements that
//! wire format as a [`MemoryStore`]-conformant surface over a real
//! [`MnemoEngine`](mnemo_core::query::MnemoEngine), so any AMP-speaking
//! client can drive mnemo's embedded DuckDB backend unchanged.
//!
//! ## What maps where
//!
//! | AMP op | Engine call |
//! |---|---|
//! | `remember` | [`MnemoEngine::remember`](mnemo_core::query::MnemoEngine::remember) |
//! | `recall` | [`MnemoEngine::recall`](mnemo_core::query::MnemoEngine::recall) (top-k, default 5) |
//! | `forget` | [`MnemoEngine::forget`](mnemo_core::query::MnemoEngine::forget) |
//! | `merge` | thin composition: `remember` (consolidated record) + `forget` (Consolidate) — **not** the branch-timeline `engine.merge` |
//! | `expire` | thin composition: set `expires_at` + `run_ttl_sweep` — there is no `engine.expire` |
//!
//! ## Pieces
//!
//! - [`wire`] — the AMP envelope, result, and JSON-Schema 2020-12.
//! - [`store`] — the [`MemoryStore`] trait + [`MnemoAmpStore`] engine impl.
//! - [`router`] — the fan-out [`AmpRouter`] + RRF / max fusion.
//! - [`approval`] — the HITL diff-and-approve hook ([`ApprovalHook`]),
//!   which gates long-term writes and records approvals in mnemo's
//!   hash-chained audit log.
//!
//! ## Honest scope
//!
//! This is a *wire-format + surface* adapter. AMP transport binding
//! (HTTP / stdio framing, `.well-known` schema discovery) is left to
//! the embedding application; the crate provides the schema document
//! and the typed envelope so that binding is mechanical.

pub mod approval;
pub mod error;
pub mod router;
pub mod store;
pub mod wire;

pub use approval::{Approval, ApprovalHook, AutoApprove, ClosureApprove, WriteDiff};
pub use error::AmpError;
pub use router::{AmpRouter, max_fuse, rrf_fuse};
pub use store::{DEFAULT_TOP_K, MemoryStore, MnemoAmpStore};
pub use wire::{AmpEnvelope, AmpHit, AmpMemoryType, AmpOp, AmpResult, schema};
