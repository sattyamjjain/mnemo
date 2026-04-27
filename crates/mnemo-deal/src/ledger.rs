//! Append-only deal ledger (v0.4.0 P1-5).

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::envelope::DealEnvelope;

/// Index into the ledger. Stable across replays — appending an
/// envelope at offset N does not reshuffle earlier offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct LedgerOffset(pub u64);

#[derive(Debug, Error, PartialEq)]
pub enum LedgerError {
    #[error("envelope's prev_hash {observed:?} does not match ledger head {expected:?}")]
    BrokenChain {
        expected: [u8; 32],
        observed: [u8; 32],
    },
    #[error("ledger lock poisoned — internal invariant violated")]
    LockPoisoned,
}

pub trait DealLedger: Send + Sync {
    fn append(&self, e: DealEnvelope) -> Result<LedgerOffset, LedgerError>;
    fn replay(&self, range: std::ops::Range<u64>) -> Vec<DealEnvelope>;
    fn head_hash(&self) -> [u8; 32];
    fn len(&self) -> u64;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// In-memory implementation. Production deployments swap in a
/// disk-backed ledger (sled, redb, or DuckDB). Same trait so the
/// host code is implementation-agnostic.
#[derive(Default)]
pub struct InMemoryDealLedger {
    rows: Mutex<Vec<DealEnvelope>>,
    head: Mutex<[u8; 32]>,
}

impl InMemoryDealLedger {
    pub fn new() -> Self {
        Self::default()
    }
}

impl DealLedger for InMemoryDealLedger {
    fn append(&self, e: DealEnvelope) -> Result<LedgerOffset, LedgerError> {
        let mut head = self.head.lock().map_err(|_| LedgerError::LockPoisoned)?;
        if e.prev_hash != *head {
            return Err(LedgerError::BrokenChain {
                expected: *head,
                observed: e.prev_hash,
            });
        }
        let next_head = e.next_prev_hash();
        let mut rows = self.rows.lock().map_err(|_| LedgerError::LockPoisoned)?;
        let offset = LedgerOffset(rows.len() as u64);
        rows.push(e);
        *head = next_head;
        Ok(offset)
    }

    fn replay(&self, range: std::ops::Range<u64>) -> Vec<DealEnvelope> {
        let rows = match self.rows.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let lo = range.start as usize;
        let hi = (range.end as usize).min(rows.len());
        if lo >= hi {
            return Vec::new();
        }
        rows[lo..hi].to_vec()
    }

    fn head_hash(&self) -> [u8; 32] {
        self.head.lock().map(|g| *g).unwrap_or([0u8; 32])
    }

    fn len(&self) -> u64 {
        self.rows.lock().map(|g| g.len() as u64).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    fn append_one(
        ledger: &InMemoryDealLedger,
        key: &[u8; 32],
        prev: [u8; 32],
        i: u64,
    ) -> DealEnvelope {
        let e = DealEnvelope::sign(
            format!("buyer-{i}"),
            format!("seller-{i}"),
            serde_json::json!({"i": i}),
            prev,
            SystemTime::UNIX_EPOCH + Duration::from_secs(i),
            key,
        )
        .unwrap();
        ledger.append(e.clone()).unwrap();
        e
    }

    #[test]
    fn append_advances_offset_and_head() {
        let ledger = InMemoryDealLedger::new();
        let key = [7u8; 32];
        let mut prev = [0u8; 32];
        for i in 0..3 {
            let e = append_one(&ledger, &key, prev, i);
            prev = e.next_prev_hash();
        }
        assert_eq!(ledger.len(), 3);
        assert_eq!(ledger.head_hash(), prev);
    }

    #[test]
    fn broken_chain_is_rejected() {
        let ledger = InMemoryDealLedger::new();
        let key = [7u8; 32];
        let _e0 = append_one(&ledger, &key, [0u8; 32], 0);
        // Build a second envelope that doesn't reference the head;
        // appending must fail.
        let bad = DealEnvelope::sign(
            "x",
            "y",
            serde_json::json!({}),
            [0u8; 32], // wrong prev — head moved
            SystemTime::UNIX_EPOCH,
            &key,
        )
        .unwrap();
        let err = ledger.append(bad).unwrap_err();
        assert!(matches!(err, LedgerError::BrokenChain { .. }));
    }

    #[test]
    fn replay_range_returns_subset() {
        let ledger = InMemoryDealLedger::new();
        let key = [7u8; 32];
        let mut prev = [0u8; 32];
        for i in 0..5 {
            let e = append_one(&ledger, &key, prev, i);
            prev = e.next_prev_hash();
        }
        let mid = ledger.replay(1..4);
        assert_eq!(mid.len(), 3);
        assert_eq!(mid[0].buyer, "buyer-1");
        assert_eq!(mid[2].buyer, "buyer-3");
    }
}
