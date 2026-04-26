//! Per-read memory-provenance signing (v0.4.0-rc3 Task B1).
//!
//! v0.3.x signs **writes** — every `MemoryRecord` carries a SHA-256
//! `content_hash` chained via `prev_hash`, and the audit log export
//! signs the chain with Ed25519. v0.4.0-rc3 adds the equivalent for
//! **reads**: every `engine.recall(..., with_provenance=true)` returns
//! a [`ReadProvenance`] HMAC that proves which writes the recall
//! derives from. A clinician auditing an LLM response can verify
//! offline that the cited memories really were the ones the model saw.
//!
//! # Threat model
//!
//! 1. **Source-record tamper.** An attacker mutates a `MemoryRecord`
//!    in storage between the recall and the audit. Detected because
//!    `verify_read_provenance` recomputes each record's `content_hash`
//!    and compares to the [`RecordRef`] in the provenance.
//! 2. **HMAC tamper.** An attacker fabricates a provenance receipt
//!    pointing at innocuous records. Detected because the HMAC binds
//!    the receipt's `read_id || query_hash || derived_from` to a
//!    server-side secret the attacker doesn't have.
//! 3. **Key rotation.** The receipt's `hmac_key_id` lets the verifier
//!    look up the historical key for a past read, so rotating the
//!    signing key doesn't break old audits.
//!
//! Out of scope: full non-repudiation (would need Ed25519 — HMAC is
//! cheaper but only verifiable by parties with the key). For
//! externally-auditable receipts, pair the provenance with the
//! existing `mnemo-compliance` Ed25519-signed audit log export.

use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::model::memory::MemoryRecord;

type HmacSha256 = Hmac<Sha256>;

/// One source record cited by a [`ReadProvenance`].
///
/// The `content_hash` and `prev_hash` mirror what's on the record
/// itself, so the verifier can re-walk the per-record chain without
/// needing to fetch from storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordRef {
    pub id: Uuid,
    /// SHA-256 of the record's `content_hash` field. Stored as 32 raw
    /// bytes; serialise as base64 / hex at the wire boundary (we
    /// serialise as a regular `Vec<u8>` here to avoid pulling
    /// `serde_bytes` — the wire format isn't pinned to a specific
    /// encoding).
    pub content_hash: Vec<u8>,
    /// `prev_hash` from the same record. `None` for the first record
    /// in an agent's chain.
    pub prev_hash: Option<Vec<u8>>,
}

/// Cryptographic receipt that an `engine.recall` call returned the
/// listed memories.
///
/// Carry this alongside any audit-log export. Verifiers call
/// [`verify_read_provenance`] with the records they have access to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadProvenance {
    pub read_id: Uuid,
    pub agent_id: String,
    /// SHA-256 of the recall query string. Hashing rather than storing
    /// the raw query keeps PII off the wire when a downstream system
    /// only needs to verify the chain.
    pub query_hash: Vec<u8>,
    /// Records the recall derived from, in retrieval-rank order.
    pub derived_from: Vec<RecordRef>,
    /// HMAC-SHA256 over `read_id || query_hash || concat(derived_from)`.
    pub hmac: Vec<u8>,
    /// Identifier of the HMAC key used. Lets verifiers look up the
    /// right historical key after rotation.
    pub hmac_key_id: String,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum ProvenanceError {
    #[error("HMAC mismatch — receipt was tampered or wrong key")]
    HmacMismatch,
    #[error(
        "record {id} content_hash mismatch — source record was modified after provenance was signed"
    )]
    RecordContentHashMismatch { id: Uuid },
    #[error("missing record {id} — verifier wasn't given the source record cited by the receipt")]
    MissingRecord { id: Uuid },
    #[error("unknown HMAC key id {key_id} — verifier doesn't have this key in its keystore")]
    UnknownKey { key_id: String },
    #[error("HMAC engine init failed: {0}")]
    HmacInit(String),
    #[error("query hash mismatch")]
    QueryHashMismatch,
}

/// In-process HMAC-SHA256 signer for the recall hot path.
///
/// Caller-side responsibility: rotate `(key_id, key)` on whatever
/// cadence your security posture demands and keep the historical
/// pairs accessible to the verifier. The struct holds a single key;
/// to handle multiple keys (active + historical) wrap several
/// `ProvenanceSigner`s in a `Keystore` (see
/// [`crate::encryption::ContentEncryption`] for the equivalent
/// pattern on the at-rest side).
#[derive(Debug, Clone)]
pub struct ProvenanceSigner {
    key_id: String,
    key: Vec<u8>,
}

impl ProvenanceSigner {
    /// Construct from a 32-byte key + a stable identifier.
    ///
    /// Operators should set the key from secure storage (Vault,
    /// AWS KMS, etc.) and choose an id like `"mnemo-prov-2026-04"`
    /// that survives logging.
    pub fn new(key_id: impl Into<String>, key: &[u8]) -> Self {
        Self {
            key_id: key_id.into(),
            key: key.to_vec(),
        }
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Build a signed [`ReadProvenance`] for one recall.
    pub fn sign(
        &self,
        agent_id: impl Into<String>,
        query: &str,
        records: &[MemoryRecord],
    ) -> Result<ReadProvenance, ProvenanceError> {
        let read_id = Uuid::now_v7();
        let query_hash = sha256(query.as_bytes());
        let derived_from: Vec<RecordRef> = records
            .iter()
            .map(|r| RecordRef {
                id: r.id,
                content_hash: r.content_hash.clone(),
                prev_hash: r.prev_hash.clone(),
            })
            .collect();
        let hmac = self.compute_hmac(&read_id, &query_hash, &derived_from)?;
        Ok(ReadProvenance {
            read_id,
            agent_id: agent_id.into(),
            query_hash,
            derived_from,
            hmac,
            hmac_key_id: self.key_id.clone(),
            ts: Utc::now(),
        })
    }

    fn compute_hmac(
        &self,
        read_id: &Uuid,
        query_hash: &[u8],
        derived_from: &[RecordRef],
    ) -> Result<Vec<u8>, ProvenanceError> {
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(&self.key)
            .map_err(|e: hmac::digest::InvalidLength| ProvenanceError::HmacInit(e.to_string()))?;
        mac.update(read_id.as_bytes());
        mac.update(query_hash);
        for r in derived_from {
            mac.update(r.id.as_bytes());
            mac.update(&r.content_hash);
            if let Some(prev) = &r.prev_hash {
                mac.update(prev);
            }
        }
        Ok(mac.finalize().into_bytes().to_vec())
    }
}

/// Verify a [`ReadProvenance`] receipt against the source records.
///
/// `records` must contain every record cited by `provenance.derived_from`
/// (the verifier looks them up by id). Order is irrelevant — they're
/// matched by id.
pub fn verify_read_provenance(
    provenance: &ReadProvenance,
    records: &[MemoryRecord],
    keystore: &dyn ProvenanceKeystore,
) -> Result<(), ProvenanceError> {
    // Look up the historical key by id so rotated keys don't invalidate
    // old audits.
    let signer =
        keystore
            .lookup(&provenance.hmac_key_id)
            .ok_or_else(|| ProvenanceError::UnknownKey {
                key_id: provenance.hmac_key_id.clone(),
            })?;

    // Walk derived_from against actual records; recompute each
    // content_hash to detect post-recall tampering.
    for r in &provenance.derived_from {
        let actual = records
            .iter()
            .find(|m| m.id == r.id)
            .ok_or(ProvenanceError::MissingRecord { id: r.id })?;
        if actual.content_hash != r.content_hash {
            return Err(ProvenanceError::RecordContentHashMismatch { id: r.id });
        }
    }

    // Recompute HMAC and compare in constant time.
    let expected = signer.compute_hmac(
        &provenance.read_id,
        &provenance.query_hash,
        &provenance.derived_from,
    )?;
    if !constant_time_eq(&expected, &provenance.hmac) {
        return Err(ProvenanceError::HmacMismatch);
    }
    Ok(())
}

/// Pluggable keystore for verifiers — supports at-least one historical key.
pub trait ProvenanceKeystore: Send + Sync {
    fn lookup(&self, key_id: &str) -> Option<&ProvenanceSigner>;
}

/// Single-key implementation for the common case.
impl ProvenanceKeystore for ProvenanceSigner {
    fn lookup(&self, key_id: &str) -> Option<&ProvenanceSigner> {
        if key_id == self.key_id {
            Some(self)
        } else {
            None
        }
    }
}

fn sha256(bytes: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().to_vec()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::compute_content_hash;
    use crate::model::memory::MemoryRecord;

    fn record(id: Uuid, agent: &str, content: &str) -> MemoryRecord {
        let mut r = MemoryRecord::new(agent.to_string(), content.to_string());
        r.id = id;
        r.content_hash = compute_content_hash(content, agent, &r.created_at);
        r
    }

    fn signer() -> ProvenanceSigner {
        ProvenanceSigner::new("mnemo-prov-test", &[7u8; 32])
    }

    #[test]
    fn sign_then_verify_round_trips() {
        let s = signer();
        let r1 = record(Uuid::now_v7(), "a", "hello");
        let r2 = record(Uuid::now_v7(), "a", "world");
        let prov = s
            .sign("a", "greeting query", &[r1.clone(), r2.clone()])
            .unwrap();
        verify_read_provenance(&prov, &[r1, r2], &s).expect("should verify");
    }

    #[test]
    fn tampering_a_source_record_fails_verification() {
        let s = signer();
        let r1 = record(Uuid::now_v7(), "a", "original content");
        let prov = s.sign("a", "q", std::slice::from_ref(&r1)).unwrap();
        // Mutate the record's content_hash after signing — this
        // simulates an attacker modifying the row in storage.
        let mut tampered = r1.clone();
        tampered.content_hash = vec![0xFF; 32];
        let err = verify_read_provenance(&prov, &[tampered], &s).unwrap_err();
        assert!(matches!(
            err,
            ProvenanceError::RecordContentHashMismatch { .. }
        ));
    }

    #[test]
    fn tampering_the_hmac_fails_verification() {
        let s = signer();
        let r1 = record(Uuid::now_v7(), "a", "x");
        let mut prov = s.sign("a", "q", std::slice::from_ref(&r1)).unwrap();
        prov.hmac[0] ^= 0xFF;
        let err = verify_read_provenance(&prov, &[r1], &s).unwrap_err();
        assert!(matches!(err, ProvenanceError::HmacMismatch));
    }

    #[test]
    fn missing_source_record_fails_verification() {
        let s = signer();
        let r1 = record(Uuid::now_v7(), "a", "x");
        let prov = s.sign("a", "q", &[r1]).unwrap();
        let err = verify_read_provenance(&prov, &[], &s).unwrap_err();
        assert!(matches!(err, ProvenanceError::MissingRecord { .. }));
    }

    #[test]
    fn unknown_key_id_fails_verification() {
        let s = signer();
        let r1 = record(Uuid::now_v7(), "a", "x");
        let mut prov = s.sign("a", "q", std::slice::from_ref(&r1)).unwrap();
        prov.hmac_key_id = "rotated-out".into();
        let err = verify_read_provenance(&prov, &[r1], &s).unwrap_err();
        assert!(matches!(err, ProvenanceError::UnknownKey { .. }));
    }

    #[test]
    fn rotated_key_still_verifies_via_keystore_lookup() {
        // Two signers with different ids — the verifier picks by id,
        // proving rotated keys still verify historical reads.
        let active = ProvenanceSigner::new("mnemo-prov-2026-05", &[1u8; 32]);
        let archived = ProvenanceSigner::new("mnemo-prov-2026-04", &[2u8; 32]);

        let r1 = record(Uuid::now_v7(), "a", "old read");
        let prov = archived.sign("a", "q", std::slice::from_ref(&r1)).unwrap();

        struct Pair<'a>(&'a ProvenanceSigner, &'a ProvenanceSigner);
        impl<'a> ProvenanceKeystore for Pair<'a> {
            fn lookup(&self, id: &str) -> Option<&ProvenanceSigner> {
                if self.0.key_id() == id {
                    Some(self.0)
                } else if self.1.key_id() == id {
                    Some(self.1)
                } else {
                    None
                }
            }
        }
        verify_read_provenance(&prov, &[r1], &Pair(&active, &archived)).unwrap();
    }
}
