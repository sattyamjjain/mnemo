//! Chain-verification + dispute reporting (v0.4.0 P1-5).

use serde::{Deserialize, Serialize};

use crate::envelope::DealEnvelope;
use crate::ledger::LedgerOffset;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisputeReport {
    pub divergent_offset: LedgerOffset,
    pub expected_hash: [u8; 32],
    pub actual_hash: [u8; 32],
}

/// Walk a stream of envelopes and produce a [`DisputeReport`] at the
/// first offset where:
///
/// 1. The envelope's `prev_hash` does not match the running chain
///    head, OR
/// 2. The envelope's HMAC fails verification under `key`.
///
/// Returns `None` if every envelope verifies.
pub fn verify_chain(envelopes: &[DealEnvelope], key: &[u8]) -> Option<DisputeReport> {
    let mut head = [0u8; 32];
    for (i, e) in envelopes.iter().enumerate() {
        if e.prev_hash != head {
            return Some(DisputeReport {
                divergent_offset: LedgerOffset(i as u64),
                expected_hash: head,
                actual_hash: e.prev_hash,
            });
        }
        if e.verify_hmac(key).is_err() {
            return Some(DisputeReport {
                divergent_offset: LedgerOffset(i as u64),
                expected_hash: e.hmac,
                actual_hash: [0u8; 32],
            });
        }
        head = e.next_prev_hash();
    }
    None
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    fn build_chain(n: u64, key: &[u8; 32]) -> Vec<DealEnvelope> {
        let mut out = Vec::with_capacity(n as usize);
        let mut prev = [0u8; 32];
        for i in 0..n {
            let e = DealEnvelope::sign(
                format!("buyer-{i}"),
                format!("seller-{i}"),
                serde_json::json!({"i": i}),
                prev,
                SystemTime::UNIX_EPOCH + Duration::from_secs(i),
                key,
            )
            .unwrap();
            prev = e.next_prev_hash();
            out.push(e);
        }
        out
    }

    #[test]
    fn intact_chain_verifies() {
        let key = [9u8; 32];
        let chain = build_chain(50, &key);
        assert!(verify_chain(&chain, &key).is_none());
    }

    #[test]
    fn tampered_terms_pinpoint_offset() {
        let key = [9u8; 32];
        let mut chain = build_chain(10, &key);
        // Flip the terms on offset 4 — verify_hmac will reject.
        chain[4].terms = serde_json::json!({"i": 99999});
        let report = verify_chain(&chain, &key).expect("should detect");
        assert_eq!(report.divergent_offset, LedgerOffset(4));
    }

    #[test]
    fn broken_prev_hash_is_caught_before_hmac() {
        let key = [9u8; 32];
        let mut chain = build_chain(10, &key);
        chain[3].prev_hash = [0xFF; 32];
        let report = verify_chain(&chain, &key).expect("should detect");
        assert_eq!(report.divergent_offset, LedgerOffset(3));
    }
}
