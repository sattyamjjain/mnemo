//! Write-time provenance — a tamper-evident record of *who wrote each memory
//! under what authority*.
//!
//! This complements the read-time receipt in [`crate::provenance`]
//! (`ReadProvenance`, which proves which records a recall cited). A
//! [`WriteProvenance`] is recorded at REMEMBER / SHARE time and captures, per
//! memory:
//!
//! - the writing **principal**,
//! - the **capability** under which the write was authorised
//!   ([`crate::model::capability::Capability`] id),
//! - the **session / trace id**,
//! - a **timestamp**,
//!
//! chained by hash so the whole write history is tamper-evident. It exists so
//! that after a poisoning incident the store can be cleaned **by principal or by
//! session** (FORGET BY PROVENANCE) instead of wiped — targeted remediation
//! instead of a reset.
//!
//! Chain scheme: `content_hash = SHA-256(memory_id ‖ principal ‖ capability_id ‖
//! session_id ‖ op ‖ authored_at ‖ prev_hash)`, and each record's `prev_hash` is
//! the previous record's `content_hash`. Tampering with any field, or reordering
//! / deleting a record, breaks the chain at [`verify_provenance_chain`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::hash::ChainVerificationResult;

/// The write operation a provenance record attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteOp {
    Remember,
    Share,
}

impl WriteOp {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            WriteOp::Remember => b"remember",
            WriteOp::Share => b"share",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            WriteOp::Remember => "remember",
            WriteOp::Share => "share",
        }
    }
}

/// A write-time flag recorded on a provenance record. Flags are **hashed into**
/// the record's `content_hash`, so a flag cannot be stripped without breaking the
/// chain — the security signal is tamper-evident, not advisory metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteFlag {
    /// The written content has the SHAPE of a provider-returned opaque reasoning
    /// payload (arXiv:2608.09867) — see [`crate::opaque_reasoning`]. Shape only:
    /// this is NOT a proof the payload contains a secret. Recorded so such a write
    /// can be found and revoked (by principal or session) later.
    OpaqueReasoningPayload,
}

impl WriteFlag {
    pub fn as_str(self) -> &'static str {
        match self {
            WriteFlag::OpaqueReasoningPayload => "opaque_reasoning_payload",
        }
    }

    /// Parse from the stored string name. Unknown values are ignored (`None`) so
    /// a forward-compatible reader never fails on a flag it does not know. (Named
    /// `from_name`, not `from_str`, since it returns `Option` and does not follow
    /// the `std::str::FromStr` `Result` contract.)
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "opaque_reasoning_payload" => Some(WriteFlag::OpaqueReasoningPayload),
            _ => None,
        }
    }
}

/// Serialize a flag set to the single-column storage form: a comma-joined list
/// of stable string names, sorted+deduped for a deterministic representation.
/// Empty set → empty string.
pub fn flags_to_storage(flags: &[WriteFlag]) -> String {
    let mut names: Vec<&'static str> = flags.iter().map(|f| f.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    names.join(",")
}

/// Parse the storage form back to a flag set (sorted+deduped; unknown names
/// skipped). Empty/whitespace → empty.
pub fn flags_from_storage(s: &str) -> Vec<WriteFlag> {
    let mut out: Vec<WriteFlag> = s
        .split(',')
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .filter_map(WriteFlag::from_name)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// One tamper-evident provenance record for a single memory write.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteProvenance {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub principal: String,
    /// The capability the write was authorised under, if any.
    pub capability_id: Option<Uuid>,
    /// Session / trace id, so a whole session's writes can be revoked together.
    pub session_id: Option<String>,
    pub op: WriteOp,
    pub authored_at: DateTime<Utc>,
    /// Write-time flags (e.g. an opaque-reasoning-payload shape match). Hashed
    /// into `content_hash`, so a flag is tamper-evident.
    #[serde(default)]
    pub flags: Vec<WriteFlag>,
    /// Previous record's `content_hash`; `None` for the first record.
    pub prev_hash: Option<Vec<u8>>,
    pub content_hash: Vec<u8>,
}

/// Deterministic content hash over the provenance fields + `flags` + `prev_hash`.
///
/// `flags` is folded in via its canonical storage form (sorted+deduped), so an
/// empty flag set contributes nothing — a pre-flags record and a no-flags record
/// hash identically, which keeps older records verifiable.
#[allow(clippy::too_many_arguments)]
pub fn compute_provenance_hash(
    memory_id: &Uuid,
    principal: &str,
    capability_id: &Option<Uuid>,
    session_id: &Option<String>,
    op: WriteOp,
    authored_at: &DateTime<Utc>,
    flags: &[WriteFlag],
    prev_hash: Option<&[u8]>,
) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(memory_id.as_bytes());
    h.update(principal.as_bytes());
    if let Some(cid) = capability_id {
        h.update(cid.as_bytes());
    }
    if let Some(sid) = session_id {
        h.update(sid.as_bytes());
    }
    h.update(op.as_bytes());
    h.update(authored_at.to_rfc3339().as_bytes());
    // Empty flag set → empty string → no bytes added (older records still hash
    // the same). Non-empty is order-independent via the sorted storage form.
    let flag_repr = flags_to_storage(flags);
    if !flag_repr.is_empty() {
        h.update(flag_repr.as_bytes());
    }
    if let Some(p) = prev_hash {
        h.update(p);
    }
    h.finalize().to_vec()
}

impl WriteProvenance {
    /// Build a provenance record chained onto `prev_hash`, computing its
    /// `content_hash`. `prev_hash` is the previous record's `content_hash`
    /// (`None` for the first record in the store's chain).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        memory_id: Uuid,
        principal: impl Into<String>,
        capability_id: Option<Uuid>,
        session_id: Option<String>,
        op: WriteOp,
        flags: Vec<WriteFlag>,
        prev_hash: Option<Vec<u8>>,
    ) -> Self {
        let principal = principal.into();
        let authored_at = Utc::now();
        let mut flags = flags;
        flags.sort_unstable();
        flags.dedup();
        let content_hash = compute_provenance_hash(
            &memory_id,
            &principal,
            &capability_id,
            &session_id,
            op,
            &authored_at,
            &flags,
            prev_hash.as_deref(),
        );
        Self {
            id: Uuid::now_v7(),
            memory_id,
            principal,
            capability_id,
            session_id,
            op,
            authored_at,
            flags,
            prev_hash,
            content_hash,
        }
    }

    /// Recompute this record's `content_hash` from its fields and compare
    /// (constant-time) to the stored value. `false` = the record was mutated.
    pub fn content_hash_valid(&self) -> bool {
        let expected = compute_provenance_hash(
            &self.memory_id,
            &self.principal,
            &self.capability_id,
            &self.session_id,
            self.op,
            &self.authored_at,
            &self.flags,
            self.prev_hash.as_deref(),
        );
        bool::from(expected.ct_eq(&self.content_hash))
    }
}

/// Verify an ordered provenance chain: every record's `content_hash` must match
/// its fields, and every `prev_hash` must equal the previous record's
/// `content_hash`. Reordering, deleting, or mutating any record breaks it.
pub fn verify_provenance_chain(records: &[WriteProvenance]) -> ChainVerificationResult {
    let mut verified = 0;
    for (i, rec) in records.iter().enumerate() {
        if !rec.content_hash_valid() {
            return ChainVerificationResult {
                valid: false,
                total_records: records.len(),
                verified_records: verified,
                first_broken_at: Some(rec.id),
                error_message: Some(format!("provenance content hash mismatch at {}", rec.id)),
            };
        }
        let expected_prev = if i == 0 {
            None
        } else {
            Some(records[i - 1].content_hash.as_slice())
        };
        if rec.prev_hash.as_deref() != expected_prev {
            return ChainVerificationResult {
                valid: false,
                total_records: records.len(),
                verified_records: verified,
                first_broken_at: Some(rec.id),
                error_message: Some(format!("provenance chain link broken at {}", rec.id)),
            };
        }
        verified += 1;
    }
    ChainVerificationResult {
        valid: true,
        total_records: records.len(),
        verified_records: verified,
        first_broken_at: None,
        error_message: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(n: usize, principal: &str) -> Vec<WriteProvenance> {
        let mut out: Vec<WriteProvenance> = Vec::new();
        for _ in 0..n {
            let prev = out.last().map(|r| r.content_hash.clone());
            out.push(WriteProvenance::new(
                Uuid::now_v7(),
                principal,
                None,
                Some("sess-1".to_string()),
                WriteOp::Remember,
                Vec::new(),
                prev,
            ));
        }
        out
    }

    #[test]
    fn valid_chain_verifies() {
        let recs = chain(5, "alice");
        let r = verify_provenance_chain(&recs);
        assert!(r.valid);
        assert_eq!(r.verified_records, 5);
        assert!(r.first_broken_at.is_none());
    }

    #[test]
    fn empty_chain_is_valid() {
        assert!(verify_provenance_chain(&[]).valid);
    }

    #[test]
    fn mutating_a_field_breaks_the_chain() {
        let mut recs = chain(3, "alice");
        recs[1].principal = "mallory".to_string(); // content_hash no longer matches
        let r = verify_provenance_chain(&recs);
        assert!(!r.valid);
        assert_eq!(r.first_broken_at, Some(recs[1].id));
        assert!(r.error_message.unwrap().contains("content hash mismatch"));
    }

    #[test]
    fn deleting_a_record_breaks_the_link() {
        let mut recs = chain(4, "alice");
        recs.remove(2); // now recs[2].prev_hash points at the deleted record
        let r = verify_provenance_chain(&recs);
        assert!(!r.valid);
        assert!(r.error_message.unwrap().contains("chain link broken"));
    }

    #[test]
    fn capability_and_session_are_hashed() {
        let cid = Uuid::now_v7();
        let a = WriteProvenance::new(
            Uuid::now_v7(),
            "p",
            Some(cid),
            Some("s".to_string()),
            WriteOp::Share,
            Vec::new(),
            None,
        );
        // Same memory/principal but different capability => different hash.
        let mut b = a.clone();
        b.capability_id = Some(Uuid::now_v7());
        assert!(!b.content_hash_valid());
    }

    #[test]
    fn flags_are_hashed_and_tamper_evident() {
        // An empty flag set hashes identically to a record built before flags
        // existed (empty repr contributes no bytes).
        let mid = Uuid::now_v7();
        let no_flags = WriteProvenance::new(mid, "p", None, None, WriteOp::Remember, vec![], None);
        assert!(no_flags.content_hash_valid());

        // A flagged record verifies...
        let flagged = WriteProvenance::new(
            mid,
            "p",
            None,
            None,
            WriteOp::Remember,
            vec![WriteFlag::OpaqueReasoningPayload],
            None,
        );
        assert!(flagged.content_hash_valid());

        // ...but stripping the flag off a flagged record breaks the hash.
        let mut stripped = flagged.clone();
        stripped.flags.clear();
        assert!(
            !stripped.content_hash_valid(),
            "removing a recorded flag must break the content hash"
        );
    }

    #[test]
    fn flags_storage_roundtrips_sorted_deduped() {
        let f = vec![
            WriteFlag::OpaqueReasoningPayload,
            WriteFlag::OpaqueReasoningPayload,
        ];
        let s = flags_to_storage(&f);
        assert_eq!(s, "opaque_reasoning_payload");
        assert_eq!(
            flags_from_storage(&s),
            vec![WriteFlag::OpaqueReasoningPayload]
        );
        assert!(flags_from_storage("").is_empty());
        assert!(flags_from_storage("unknown_future_flag").is_empty());
    }
}
