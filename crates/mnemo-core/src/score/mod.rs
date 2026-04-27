//! v0.4.0 (P1-4) — pluggable score lanes for hybrid recall fusion.
//!
//! The default Mnemo recall fuses four signals: vector similarity,
//! BM25 lexical, recency, and (new in v0.4.0) Ebbinghaus-style decay
//! with reinforcement. Each signal is a `ScoreLane` that maps a
//! candidate memory to a `f32` in `[0.0, 1.0]`. The fusion sums the
//! lanes with operator-tuned weights:
//!
//! ```text
//! score = 0.55 * vector
//!       + 0.20 * bm25
//!       + 0.15 * recency
//!       + 0.10 * decay
//! ```
//!
//! Letta-protocol mode (`LettaProtocolMode`) skips the decay lane so
//! parity with Letta's published numbers is preserved.

pub mod decay;

pub use decay::{DecayLane, DecayParams, decay_weight};

use std::time::SystemTime;

use crate::model::memory::MemoryRecord;

/// One scoring signal. Implementations are stateless or hold cheap
/// configuration; the recall path holds them as `Arc<dyn ScoreLane>`.
pub trait ScoreLane: Send + Sync {
    /// Bounded score in `[0.0, 1.0]`. Higher is better.
    fn score(&self, mem: &MemoryRecord, ctx: &ScoreContext) -> f32;

    /// Stable name — used in audit-log explanations + debug output.
    fn name(&self) -> &'static str;
}

/// Context the recall path threads through to every lane. Holds
/// per-query state (current time, agent's recent activity) so a lane
/// can use it without re-querying storage.
#[derive(Debug, Clone)]
pub struct ScoreContext {
    pub now: SystemTime,
    pub query_text: String,
    /// `true` when the recall request set `mode = Letta`. Lanes that
    /// must be bypassed for parity check this flag.
    pub letta_mode: bool,
}

impl ScoreContext {
    pub fn new(now: SystemTime, query_text: impl Into<String>) -> Self {
        Self {
            now,
            query_text: query_text.into(),
            letta_mode: false,
        }
    }

    pub fn with_letta_mode(mut self, on: bool) -> Self {
        self.letta_mode = on;
        self
    }
}

/// Default v0.4.0 fusion weights. Tuned against the bundled
/// LongMemEval_M sample so the bench gate doesn't regress.
pub const DEFAULT_VECTOR_WEIGHT: f32 = 0.55;
pub const DEFAULT_BM25_WEIGHT: f32 = 0.20;
pub const DEFAULT_RECENCY_WEIGHT: f32 = 0.15;
pub const DEFAULT_DECAY_WEIGHT: f32 = 0.10;

/// Sum the four lane signals using the v0.4.0 default weights.
/// Operators that override weights via `RecallRequest.hybrid_weights`
/// build their own `fuse_weighted` call site.
pub fn fuse_default(vector: f32, bm25: f32, recency: f32, decay: f32) -> f32 {
    fuse_weighted(
        vector,
        bm25,
        recency,
        decay,
        DEFAULT_VECTOR_WEIGHT,
        DEFAULT_BM25_WEIGHT,
        DEFAULT_RECENCY_WEIGHT,
        DEFAULT_DECAY_WEIGHT,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn fuse_weighted(
    vector: f32,
    bm25: f32,
    recency: f32,
    decay: f32,
    w_vector: f32,
    w_bm25: f32,
    w_recency: f32,
    w_decay: f32,
) -> f32 {
    (vector * w_vector + bm25 * w_bm25 + recency * w_recency + decay * w_decay).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_weights_sum_to_one() {
        let s = DEFAULT_VECTOR_WEIGHT
            + DEFAULT_BM25_WEIGHT
            + DEFAULT_RECENCY_WEIGHT
            + DEFAULT_DECAY_WEIGHT;
        assert!((s - 1.0).abs() < 1e-6, "weights sum {s} should be 1.0");
    }

    #[test]
    fn fuse_clamps_to_unit_interval() {
        // Negative inputs cannot push the fused score below 0.
        let s = fuse_default(-1.0, 0.0, 0.0, 0.0);
        assert_eq!(s, 0.0);
        // Inputs > 1.0 cannot push it above 1.0.
        let s = fuse_default(2.0, 2.0, 2.0, 2.0);
        assert_eq!(s, 1.0);
    }

    #[test]
    fn fuse_default_is_monotonic_in_each_lane() {
        let base = fuse_default(0.5, 0.5, 0.5, 0.5);
        assert!(fuse_default(0.6, 0.5, 0.5, 0.5) > base);
        assert!(fuse_default(0.5, 0.6, 0.5, 0.5) > base);
        assert!(fuse_default(0.5, 0.5, 0.6, 0.5) > base);
        assert!(fuse_default(0.5, 0.5, 0.5, 0.6) > base);
    }
}
