//! v0.4.5 — Attention-state-memory storage substrate for mnemo.
//!
//! # Anchor
//!
//! [arXiv:2605.18226](https://arxiv.org/abs/2605.18226) (Okoshi et al.,
//! Institute of Science Tokyo + Imperial College London, surfaced
//! 2026-05-19) introduces *Context Memorization*: a training-free
//! externalization of prefix attention states into a lightweight,
//! lookup-based memory of precomputed attention states. The paper's
//! framing — that lookup-based memory is the right substrate for
//! attention-state reuse — is what this crate is structured around.
//!
//! # What this crate IS
//!
//! - A typed substrate ([`AttentionStateStore`] trait) for persisting
//!   precomputed attention-state blobs by `(agent_id, prefix_hash)`
//!   key.
//! - A reference [`InMemoryAttentionStateStore`] implementation
//!   suitable for tests and short-lived sessions.
//! - A serializable [`AttentionStateRecord`] envelope carrying the
//!   blob plus metadata: producer model identity, optional TTL,
//!   created-at timestamp.
//!
//! # What this crate is NOT (v0.4.5)
//!
//! - **Not an integration with any inference runtime.** mnemo does
//!   not produce KV cache states. The blob format, model
//!   compatibility, and quantization sensitivity are the producer's
//!   responsibility — this crate stores opaque bytes.
//! - **Not a RECALL fast-path.** The substrate sits orthogonal to
//!   the existing semantic + BM25 + graph hybrid retrieval. A
//!   future v0.5.x row may wire `attention_state.get` into a
//!   prefix-matched fast-path; today's surface is the store + the
//!   two MCP tools.
//! - **Not a stability claim on the wire format.** The
//!   [`AttentionStateRecord`] schema is starter; pin the mnemo
//!   minor version if relying on byte-level layout.
//! - **Not encryption-at-rest.** The in-memory store keeps blobs as
//!   `Vec<u8>` in memory. The MCP tool layer wraps every blob with
//!   the existing `mnemo-core::encryption::ContentEncryption`
//!   helper before handing it off; the encryption boundary is at
//!   the tool layer, not at this storage trait.
//!
//! # Honest framing
//!
//! Implementing the paper's *complete* mechanism — Context
//! Memorization end-to-end — requires (a) a runtime that exposes
//! prefix-state extraction, (b) prefix-hash semantics that match
//! across quantization / context-length / model-version, (c) a
//! consumer that re-injects the cached state into the next
//! generation. None of those are inside mnemo's scope today, and
//! none of them are claimed by this crate.
//!
//! What v0.4.5 ships is the **substrate** the paper anchors against:
//! a typed, lookup-based store accessible through the existing MCP
//! tool surface, ready for a producer + consumer to bind against in
//! a future release.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Errors surfaced by the attention-state store.
#[derive(Debug, Error)]
pub enum AttentionStateError {
    /// Returned when a `get` query addresses a (agent_id, prefix_hash)
    /// the store has no record of.
    #[error("attention state not found for agent_id={agent_id} prefix_hash={prefix_hash}")]
    NotFound {
        agent_id: String,
        prefix_hash: String,
    },
    /// Returned when the underlying storage layer fails. Carries the
    /// upstream message for diagnostics; does not preserve the
    /// underlying error type.
    #[error("attention state storage error: {0}")]
    Storage(String),
}

/// One stored attention-state record. Serializable so the wire
/// format is stable across SDKs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttentionStateRecord {
    /// Time-sortable identifier the store assigns on put.
    pub id: Uuid,
    /// Owning agent. Matches mnemo's standard `agent_id` scoping.
    pub agent_id: String,
    /// Caller-chosen prefix identity. Convention: hex-encoded SHA-256
    /// of the producer's prompt tokens, but the store treats it as
    /// an opaque key.
    pub prefix_hash: String,
    /// Producer-chosen model identifier (e.g. `"claude-sonnet-4.6@bf16-tp1"`).
    /// Stored as metadata so a future consumer can refuse a state
    /// blob produced under incompatible quantization.
    pub model: Option<String>,
    /// Opaque attention-state blob. Format is the producer's
    /// responsibility; this crate does not parse or validate.
    pub state_blob: Vec<u8>,
    /// SHA-256 of `state_blob`. Filled in by `put`; lets callers
    /// verify blob integrity end-to-end without holding the bytes.
    pub blob_sha256_hex: String,
    /// Producer-chosen TTL in seconds. `None` means no expiry. The
    /// in-memory reference store does not honour expiry — that's
    /// the operator's responsibility at the tool / engine layer.
    pub ttl_seconds: Option<u64>,
    /// RFC3339 timestamp the store assigned on put.
    pub created_at: String,
}

impl AttentionStateRecord {
    /// Compute the SHA-256 of `state_blob` and return the hex digest.
    /// Exposed so callers can verify blob integrity end-to-end
    /// without the store handing back the bytes.
    pub fn compute_blob_sha256_hex(state_blob: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(state_blob);
        hex::encode(h.finalize())
    }
}

/// Pluggable storage trait for attention-state records. The intent
/// is the same shape as `mnemo-core::storage::StorageBackend`: a
/// typed contract plus pluggable implementations (in-memory for
/// tests, DuckDB / PostgreSQL for production once they land in a
/// future minor).
#[async_trait]
pub trait AttentionStateStore: Send + Sync {
    /// Insert or replace an attention-state record under
    /// `(agent_id, prefix_hash)`. The store assigns `id`,
    /// `blob_sha256_hex`, and `created_at`; the returned record
    /// carries those values.
    async fn put(
        &self,
        agent_id: String,
        prefix_hash: String,
        state_blob: Vec<u8>,
        model: Option<String>,
        ttl_seconds: Option<u64>,
    ) -> Result<AttentionStateRecord, AttentionStateError>;

    /// Look up the most-recent record for `(agent_id, prefix_hash)`,
    /// or `None` if no record exists.
    async fn get(
        &self,
        agent_id: &str,
        prefix_hash: &str,
    ) -> Result<Option<AttentionStateRecord>, AttentionStateError>;

    /// Delete every record owned by `agent_id`. Returns the number
    /// of records removed. Used by `mnemo.forget_subject` to honour
    /// DPDPA / GDPR subject-erasure requests over the attention-state
    /// substrate alongside the memory substrate.
    async fn delete_for_agent(&self, agent_id: &str) -> Result<usize, AttentionStateError>;
}

/// Reference in-memory implementation of [`AttentionStateStore`].
/// Suitable for tests and short-lived sessions. Production
/// deployments should swap to a persistent store in a future
/// minor.
#[derive(Default, Debug)]
pub struct InMemoryAttentionStateStore {
    inner: RwLock<HashMap<(String, String), AttentionStateRecord>>,
}

impl InMemoryAttentionStateStore {
    /// Build an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap into an `Arc` for engine attachment.
    pub fn into_arc(self) -> Arc<dyn AttentionStateStore> {
        Arc::new(self)
    }
}

#[async_trait]
impl AttentionStateStore for InMemoryAttentionStateStore {
    async fn put(
        &self,
        agent_id: String,
        prefix_hash: String,
        state_blob: Vec<u8>,
        model: Option<String>,
        ttl_seconds: Option<u64>,
    ) -> Result<AttentionStateRecord, AttentionStateError> {
        let blob_sha256_hex = AttentionStateRecord::compute_blob_sha256_hex(&state_blob);
        // Generate a deterministic ISO-8601 timestamp at second
        // resolution. Avoiding `chrono` here keeps the dep graph
        // minimal; the format matches `mnemo-core::AgentEvent.timestamp`.
        let created_at = format_now_iso8601_utc();
        let record = AttentionStateRecord {
            id: Uuid::now_v7(),
            agent_id: agent_id.clone(),
            prefix_hash: prefix_hash.clone(),
            model,
            state_blob,
            blob_sha256_hex,
            ttl_seconds,
            created_at,
        };
        let mut guard = self.inner.write().await;
        guard.insert((agent_id, prefix_hash), record.clone());
        Ok(record)
    }

    async fn get(
        &self,
        agent_id: &str,
        prefix_hash: &str,
    ) -> Result<Option<AttentionStateRecord>, AttentionStateError> {
        let guard = self.inner.read().await;
        Ok(guard
            .get(&(agent_id.to_string(), prefix_hash.to_string()))
            .cloned())
    }

    async fn delete_for_agent(&self, agent_id: &str) -> Result<usize, AttentionStateError> {
        let mut guard = self.inner.write().await;
        let before = guard.len();
        guard.retain(|(stored_agent, _), _| stored_agent != agent_id);
        Ok(before - guard.len())
    }
}

/// Tiny RFC3339-ish formatter so we don't pull in `chrono` for one
/// timestamp. Returns `YYYY-MM-DDTHH:MM:SSZ` at second resolution.
fn format_now_iso8601_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Convert epoch seconds to UTC components via integer math —
    // proleptic Gregorian, no leap-seconds. Adequate for an audit
    // timestamp on a record that already carries a UUIDv7.
    let (year, month, day, hour, minute, second) = epoch_to_utc(now);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn epoch_to_utc(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let second = (secs % 60) as u32;
    let minute = ((secs / 60) % 60) as u32;
    let hour = ((secs / 3600) % 24) as u32;
    let days = secs / 86400;
    // Days since 1970-01-01 → calendar date via proleptic Gregorian.
    let mut year: i32 = 1970;
    let mut remaining = days as i64;
    loop {
        let leap = is_leap(year);
        let yd = if leap { 366 } else { 365 };
        if remaining < yd {
            break;
        }
        remaining -= yd;
        year += 1;
    }
    let mut month: u32 = 1;
    loop {
        let dim = days_in_month(year, month) as i64;
        if remaining < dim {
            break;
        }
        remaining -= dim;
        month += 1;
    }
    let day = (remaining + 1) as u32;
    (year, month, day, hour, minute, second)
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_then_get_round_trips() {
        let store = InMemoryAttentionStateStore::new();
        let stored = store
            .put(
                "agent-1".to_string(),
                "prefix-abc".to_string(),
                b"opaque-state-bytes".to_vec(),
                Some("test-model@bf16".to_string()),
                Some(3600),
            )
            .await
            .unwrap();
        assert_eq!(stored.agent_id, "agent-1");
        assert_eq!(stored.prefix_hash, "prefix-abc");
        assert_eq!(stored.state_blob, b"opaque-state-bytes".to_vec());
        assert_eq!(stored.model.as_deref(), Some("test-model@bf16"));
        assert_eq!(stored.ttl_seconds, Some(3600));
        assert_eq!(stored.blob_sha256_hex.len(), 64);

        let got = store.get("agent-1", "prefix-abc").await.unwrap();
        assert_eq!(got, Some(stored));
    }

    #[tokio::test]
    async fn get_miss_returns_none() {
        let store = InMemoryAttentionStateStore::new();
        let miss = store.get("agent-1", "never-stored").await.unwrap();
        assert!(miss.is_none());
    }

    #[tokio::test]
    async fn put_overwrites_existing_record_under_same_key() {
        let store = InMemoryAttentionStateStore::new();
        let _v1 = store
            .put(
                "agent-1".to_string(),
                "prefix-x".to_string(),
                b"v1".to_vec(),
                None,
                None,
            )
            .await
            .unwrap();
        let v2 = store
            .put(
                "agent-1".to_string(),
                "prefix-x".to_string(),
                b"v2".to_vec(),
                None,
                None,
            )
            .await
            .unwrap();
        let got = store.get("agent-1", "prefix-x").await.unwrap().unwrap();
        assert_eq!(got.state_blob, b"v2".to_vec());
        assert_eq!(got.id, v2.id);
    }

    #[tokio::test]
    async fn blob_sha256_matches_input() {
        let store = InMemoryAttentionStateStore::new();
        let blob = b"deterministic-bytes-for-sha-test".to_vec();
        let expected = AttentionStateRecord::compute_blob_sha256_hex(&blob);
        let stored = store
            .put(
                "agent-1".to_string(),
                "prefix-sha".to_string(),
                blob.clone(),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(stored.blob_sha256_hex, expected);
    }

    #[tokio::test]
    async fn agent_scoping_isolates_writes() {
        let store = InMemoryAttentionStateStore::new();
        store
            .put(
                "agent-a".to_string(),
                "shared-prefix".to_string(),
                b"a".to_vec(),
                None,
                None,
            )
            .await
            .unwrap();
        store
            .put(
                "agent-b".to_string(),
                "shared-prefix".to_string(),
                b"b".to_vec(),
                None,
                None,
            )
            .await
            .unwrap();
        let a = store
            .get("agent-a", "shared-prefix")
            .await
            .unwrap()
            .unwrap();
        let b = store
            .get("agent-b", "shared-prefix")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(a.state_blob, b"a".to_vec());
        assert_eq!(b.state_blob, b"b".to_vec());
    }

    #[tokio::test]
    async fn delete_for_agent_removes_only_that_agents_records() {
        let store = InMemoryAttentionStateStore::new();
        for k in ["k1", "k2", "k3"] {
            store
                .put(
                    "agent-doomed".to_string(),
                    k.to_string(),
                    b"x".to_vec(),
                    None,
                    None,
                )
                .await
                .unwrap();
        }
        store
            .put(
                "agent-keep".to_string(),
                "k1".to_string(),
                b"k".to_vec(),
                None,
                None,
            )
            .await
            .unwrap();

        let removed = store.delete_for_agent("agent-doomed").await.unwrap();
        assert_eq!(removed, 3);
        assert!(store.get("agent-doomed", "k1").await.unwrap().is_none());
        assert!(store.get("agent-keep", "k1").await.unwrap().is_some());
    }
}
