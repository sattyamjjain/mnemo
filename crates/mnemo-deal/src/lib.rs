//! v0.4.0 (P1-5) — agent-on-agent deal ledger.
//!
//! Anthropic Project Deal (announced 2026-04-25) opens up
//! agent-on-agent commerce: one agent contracts another to perform a
//! task and the buyer's host needs a tamper-evident record of the
//! agreed terms + completion. This crate ships the substrate: a
//! chained-HMAC log of [`DealEnvelope`]s, each one signed in line
//! with the existing memory-provenance chain so audit-log export
//! emits one continuous ledger.
//!
//! Three pieces:
//!
//! 1. [`envelope::DealEnvelope`] — the minimal contract shape (who,
//!    what, when, prev_hash, hmac).
//! 2. [`ledger::DealLedger`] trait + `InMemoryDealLedger` impl —
//!    append-only log with replay over an offset range.
//! 3. [`dispute::verify_chain`] + [`dispute::DisputeReport`] — walks
//!    the chain, finds the first divergence between expected and
//!    observed hashes, and pinpoints the offset that broke.

pub mod discovery;
pub mod dispute;
pub mod envelope;
pub mod ledger;
pub mod reputation;

pub use discovery::{AgentAdvertisement, DealCapability, Ed25519PubBytes};
pub use dispute::{DisputeReport, verify_chain};
pub use envelope::{AgentId, DealEnvelope, EnvelopeError};
pub use ledger::{DealLedger, InMemoryDealLedger, LedgerError, LedgerOffset};
pub use reputation::{ReputationScore, compute_reputation};
