//! Bridge CMA `audit.jsonl` rows into mnemo's HMAC chain (v0.4.1 P0-2).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Marks a mnemo `AuditEvent` as having been produced by a CMA-Memory
/// write rather than a native `remember` call. Lives in the event's
/// `metadata` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmaSource {
    /// The Anthropic CMA-Memory beta wrote this row through the
    /// shim's `WriteThrough` path.
    CmaBeta,
    /// One-shot import of a pre-existing CMA tree.
    CmaImport,
}

impl CmaSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            CmaSource::CmaBeta => "cma_beta",
            CmaSource::CmaImport => "cma_import",
        }
    }
}

/// One CMA audit row that has been bridged into mnemo's chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgedEvent {
    pub source: CmaSource,
    pub cma_path: String,
    pub cma_op: String,
    pub bytes: u64,
    pub prev_hash: [u8; 32],
    pub bridge_hash: [u8; 32],
}

#[derive(Debug, Error, PartialEq)]
pub enum BridgeError {
    #[error("malformed CMA audit row: {0}")]
    Malformed(String),
}

/// Hash a CMA row into the chain. Pure fn so the audit-log writer
/// can compute heads without touching any keys.
pub fn bridge_event(
    source: CmaSource,
    cma_path: &str,
    cma_op: &str,
    bytes: u64,
    prev_hash: [u8; 32],
) -> BridgedEvent {
    let mut h = Sha256::new();
    h.update(prev_hash);
    h.update(source.as_str().as_bytes());
    h.update(b"|");
    h.update(cma_path.as_bytes());
    h.update(b"|");
    h.update(cma_op.as_bytes());
    h.update(b"|");
    h.update(bytes.to_be_bytes());
    let bridge_hash: [u8; 32] = h.finalize().into();
    BridgedEvent {
        source,
        cma_path: cma_path.to_string(),
        cma_op: cma_op.to_string(),
        bytes,
        prev_hash,
        bridge_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_is_deterministic_for_fixed_input() {
        let a = bridge_event(CmaSource::CmaBeta, "notes/x.md", "write", 42, [0u8; 32]);
        let b = bridge_event(CmaSource::CmaBeta, "notes/x.md", "write", 42, [0u8; 32]);
        assert_eq!(a, b);
    }

    #[test]
    fn bridge_changes_with_path() {
        let a = bridge_event(CmaSource::CmaBeta, "x.md", "write", 1, [0u8; 32]);
        let b = bridge_event(CmaSource::CmaBeta, "y.md", "write", 1, [0u8; 32]);
        assert_ne!(a.bridge_hash, b.bridge_hash);
    }

    #[test]
    fn bridge_changes_with_op() {
        let a = bridge_event(CmaSource::CmaBeta, "x.md", "write", 1, [0u8; 32]);
        let b = bridge_event(CmaSource::CmaBeta, "x.md", "delete", 1, [0u8; 32]);
        assert_ne!(a.bridge_hash, b.bridge_hash);
    }

    #[test]
    fn bridge_chains_off_prev() {
        let a = bridge_event(CmaSource::CmaBeta, "x.md", "write", 1, [0u8; 32]);
        let b = bridge_event(CmaSource::CmaBeta, "x.md", "write", 1, a.bridge_hash);
        assert_ne!(a.bridge_hash, b.bridge_hash);
        assert_eq!(b.prev_hash, a.bridge_hash);
    }
}
