//! Ebbinghaus-style decay-curve scoring (v0.4.0 P1-4).
//!
//! Reads the memory's `last_accessed_at` + `access_count` and produces
//! a score in `[floor, 1.0]` that decays exponentially with age and
//! is reinforced by access count:
//!
//! ```text
//! age_secs = max(0, now - last_accessed_at)
//! base = 0.5 ^ (age_secs / half_life_secs)
//! lift = log2(1 + access_count) * reinforcement_factor
//! weight = clamp(base + lift, floor, 1.0)
//! ```
//!
//! Defaults (from a quick LongMemEval_M sweep, not from a paper):
//! `half_life_secs = 7 * 24 * 3600` (one week), `reinforcement_factor
//! = 0.05`, `floor = 0.0`. Operators tune via
//! `RecallRequest.hybrid_weights` + a `decay_params` field on the
//! engine builder.
//!
//! Direct competitive response to YourMemory's biological-decay
//! marketing (Show HN, 2026-04-27); fused with vector + BM25 +
//! recency rather than replacing them, so we keep our hybrid edge.

use std::time::SystemTime;

use crate::model::memory::MemoryRecord;

use super::{ScoreContext, ScoreLane};

#[derive(Debug, Clone, PartialEq)]
pub struct DecayParams {
    pub half_life_secs: u64,
    pub reinforcement_factor: f32,
    pub floor: f32,
}

impl Default for DecayParams {
    fn default() -> Self {
        Self {
            half_life_secs: 7 * 24 * 3600,
            reinforcement_factor: 0.05,
            floor: 0.0,
        }
    }
}

/// Pure function the lane and ad-hoc callers (e.g. CLI inspect) use.
/// Bounded in `[floor, 1.0]`.
pub fn decay_weight(now: SystemTime, last_access: SystemTime, hits: u32, p: &DecayParams) -> f32 {
    let age_secs = now
        .duration_since(last_access)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let base = if p.half_life_secs == 0 {
        0.0
    } else {
        0.5_f32.powf(age_secs as f32 / p.half_life_secs as f32)
    };
    let lift = (1.0 + hits as f32).log2() * p.reinforcement_factor;
    (base + lift).clamp(p.floor, 1.0)
}

pub struct DecayLane {
    pub params: DecayParams,
}

impl DecayLane {
    pub fn new(params: DecayParams) -> Self {
        Self { params }
    }
}

impl Default for DecayLane {
    fn default() -> Self {
        Self::new(DecayParams::default())
    }
}

impl ScoreLane for DecayLane {
    fn score(&self, mem: &MemoryRecord, ctx: &ScoreContext) -> f32 {
        // Bypass under Letta-protocol mode for parity with Letta's
        // published recall numbers.
        if ctx.letta_mode {
            return 0.0;
        }
        // Records without a `last_accessed_at` fall back to
        // `created_at`. We parse the RFC3339 timestamps that Mnemo
        // already serializes.
        let last_str = mem.last_accessed_at.as_deref().unwrap_or(&mem.created_at);
        let Ok(last_dt) = chrono::DateTime::parse_from_rfc3339(last_str) else {
            // Malformed timestamp shouldn't tank recall; treat as
            // "infinitely old" — the floor catches it.
            return self.params.floor;
        };
        let last: SystemTime = last_dt.with_timezone(&chrono::Utc).into();
        decay_weight(ctx.now, last, mem.access_count as u32, &self.params)
    }

    fn name(&self) -> &'static str {
        "decay"
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn fresh_memory_with_zero_hits_starts_near_one() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let p = DecayParams::default();
        let w = decay_weight(now, now, 0, &p);
        assert!(w > 0.99, "fresh weight should be ~1.0, got {w}");
    }

    #[test]
    fn weight_is_monotonic_decreasing_in_age_for_fixed_hits() {
        let p = DecayParams::default();
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000_000);
        let mut prev = decay_weight(base, base, 0, &p);
        for d in [1, 60, 3600, 86_400, 604_800] {
            let later = base + Duration::from_secs(d);
            let w = decay_weight(later, base, 0, &p);
            assert!(
                w <= prev + 1e-6,
                "weight should not increase: prev={prev} w={w} d={d}"
            );
            prev = w;
        }
    }

    #[test]
    fn reinforcement_lifts_a_repeatedly_recalled_memory() {
        let p = DecayParams {
            half_life_secs: 86_400,
            reinforcement_factor: 0.1,
            floor: 0.0,
        };
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let aged = base + Duration::from_secs(86_400 * 3); // 3 days = 12.5% of fresh
        let cold = decay_weight(aged, base, 0, &p);
        let hot = decay_weight(aged, base, 32, &p);
        assert!(
            hot > cold,
            "32-hit memory should rank above same-age zero-hit: cold={cold} hot={hot}"
        );
    }

    #[test]
    fn floor_is_respected() {
        let p = DecayParams {
            half_life_secs: 1,
            reinforcement_factor: 0.0,
            floor: 0.25,
        };
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let very_old = base + Duration::from_secs(1_000_000);
        let w = decay_weight(very_old, base, 0, &p);
        assert!(
            (w - 0.25).abs() < 1e-6,
            "very old memory should clamp to floor=0.25, got {w}"
        );
    }

    #[test]
    fn letta_mode_zeros_the_lane_for_parity() {
        let lane = DecayLane::default();
        let mem = MemoryRecord::new("a".into(), "c".into());
        let ctx = ScoreContext::new(SystemTime::now(), "q").with_letta_mode(true);
        let s = lane.score(&mem, &ctx);
        assert_eq!(s, 0.0);
    }

    #[test]
    fn lane_name_is_stable() {
        assert_eq!(DecayLane::default().name(), "decay");
    }
}
