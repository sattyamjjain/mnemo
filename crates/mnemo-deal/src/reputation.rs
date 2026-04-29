//! Advisory reputation score (v0.4.1 P1-5).
//!
//! Reputation is gameable; the README's threat-model section spells
//! that out — the score is **advisory**, not a gate. The shape:
//!
//! ```text
//! score = (completed_weighted - dispute_penalty) / total_weighted
//! ```
//!
//! Older completed deals decay with a 90-day half-life so a long-
//! dormant reputation eventually falls back to neutral. One verified
//! [`crate::DisputeReport`] drops the score by ≥ 10%.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::dispute::DisputeReport;
use crate::envelope::DealEnvelope;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReputationScore {
    pub agent: String,
    pub completed: u32,
    pub disputed: u32,
    pub mean_settlement_ms: u64,
    pub score: f32,
}

impl ReputationScore {
    pub fn empty(agent: impl Into<String>) -> Self {
        Self {
            agent: agent.into(),
            completed: 0,
            disputed: 0,
            mean_settlement_ms: 0,
            score: 0.5, // neutral
        }
    }
}

const HALF_LIFE_SECS: f32 = 90.0 * 24.0 * 3600.0;

fn decay_weight(now: SystemTime, signed_at: SystemTime) -> f32 {
    let age_secs = now
        .duration_since(signed_at)
        .map(|d| d.as_secs())
        .unwrap_or(0) as f32;
    0.5_f32.powf(age_secs / HALF_LIFE_SECS)
}

/// Compute the reputation score for an agent given their history of
/// completed envelopes + the disputes filed against them.
pub fn compute_reputation(
    agent: &str,
    history: &[DealEnvelope],
    disputes: &[DisputeReport],
) -> ReputationScore {
    let now = SystemTime::now();
    let agent_envelopes: Vec<&DealEnvelope> = history
        .iter()
        .filter(|e| e.seller == agent || e.buyer == agent)
        .collect();
    let completed = agent_envelopes.len() as u32;
    let disputed = disputes.len() as u32;

    if completed == 0 {
        return ReputationScore::empty(agent);
    }

    let mut weighted_completed = 0.0f32;
    let mut weighted_total = 0.0f32;
    let mut settle_total = 0u64;
    for e in &agent_envelopes {
        let w = decay_weight(now, e.signed_at);
        weighted_completed += w;
        weighted_total += w;
        settle_total += now
            .duration_since(e.signed_at)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64;
    }

    // Dispute penalty: 10% of weighted_completed per dispute, floored at 0.
    let dispute_penalty = disputed as f32 * 0.10 * weighted_completed;
    // If every deal has decayed to near-zero weight, fall back to
    // neutral rather than dividing by ~0 (which produces NaN). The
    // floor matches the "empty history → 0.5" rule.
    let score = if weighted_total < 1e-6 {
        0.5
    } else {
        ((weighted_completed - dispute_penalty) / weighted_total).clamp(0.0, 1.0)
    };

    ReputationScore {
        agent: agent.to_string(),
        completed,
        disputed,
        mean_settlement_ms: settle_total / completed as u64,
        score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::DealEnvelope;

    fn key() -> [u8; 32] {
        [33u8; 32]
    }

    fn envelope(buyer: &str, seller: &str, signed_at: SystemTime) -> DealEnvelope {
        DealEnvelope::sign(
            buyer,
            seller,
            serde_json::json!({"price": "10 USDC"}),
            [0u8; 32],
            signed_at,
            &key(),
        )
        .unwrap()
    }

    #[test]
    fn empty_history_yields_neutral_score() {
        let r = compute_reputation("a", &[], &[]);
        assert_eq!(r.completed, 0);
        assert!((r.score - 0.5).abs() < 1e-3);
    }

    #[test]
    fn fresh_completed_deals_score_near_one() {
        let now = SystemTime::now();
        let history: Vec<DealEnvelope> = (0..5).map(|_| envelope("buyer", "seller", now)).collect();
        let r = compute_reputation("seller", &history, &[]);
        assert!(
            r.score > 0.95,
            "fresh agent should score >0.95, got {}",
            r.score
        );
        assert_eq!(r.completed, 5);
    }

    #[test]
    fn old_deals_decay_to_neutral_when_weight_is_tiny() {
        // Very old deal (decades ago): weight underflows to ~0,
        // score falls back to neutral 0.5 instead of dividing 0/0.
        let very_old = SystemTime::UNIX_EPOCH;
        let now = SystemTime::now();
        let new_history = vec![envelope("buyer", "seller", now)];
        let old_history = vec![envelope("buyer", "seller", very_old)];
        let r_new = compute_reputation("seller", &new_history, &[]);
        let r_old = compute_reputation("seller", &old_history, &[]);
        // Fresh deal scores 1.0, very-old falls back to neutral.
        assert_eq!(r_new.score, 1.0);
        assert!(
            (r_old.score - 0.5).abs() < 1e-3,
            "very-old deal should fall back to neutral 0.5, got {}",
            r_old.score
        );
    }

    #[test]
    fn dispute_drops_score_by_at_least_ten_percent() {
        let now = SystemTime::now();
        let history: Vec<DealEnvelope> = (0..5).map(|_| envelope("buyer", "seller", now)).collect();
        let no_dispute = compute_reputation("seller", &history, &[]);
        let dispute_report = DisputeReport {
            divergent_offset: crate::ledger::LedgerOffset(0),
            expected_hash: [0u8; 32],
            actual_hash: [1u8; 32],
        };
        let with_one = compute_reputation("seller", &history, &[dispute_report]);
        let drop = no_dispute.score - with_one.score;
        assert!(
            drop >= 0.09,
            "expected >=10% drop, got {drop} (no_dispute={}, with_one={})",
            no_dispute.score,
            with_one.score
        );
    }
}
