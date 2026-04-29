//! Recall budget planner (v0.4.1 P1-4).

use serde::{Deserialize, Serialize};

use super::models::{ModelId, lookup};

/// Operator-tunable budget. Built from a [`ModelId`] via
/// `ContextBudget::for_model` and overridden in
/// [`ContextBudget::with_*`] methods.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ContextBudget {
    pub model: ModelId,
    pub total_tokens: u32,
    pub system_reserve: u32,
    pub response_reserve: u32,
    /// Fraction of the post-reserves remainder reserved for memory
    /// injection. The rest is left to the conversation history.
    /// Default 0.45 — gives ~45% to memory and 55% to history on a
    /// budget that's already had system + response carved out.
    pub mem_share: f32,
}

impl ContextBudget {
    pub fn for_model(model: ModelId) -> Self {
        let w = lookup(model);
        Self {
            model,
            total_tokens: w.total_tokens,
            system_reserve: w.system_reserve,
            response_reserve: w.response_reserve,
            mem_share: 0.45,
        }
    }

    pub fn with_mem_share(mut self, share: f32) -> Self {
        self.mem_share = share.clamp(0.0, 1.0);
        self
    }

    /// Tokens available to the conversation + memory after reserves.
    pub fn available(&self) -> u32 {
        self.total_tokens
            .saturating_sub(self.system_reserve)
            .saturating_sub(self.response_reserve)
    }

    pub fn memory_budget(&self) -> u32 {
        (self.available() as f32 * self.mem_share) as u32
    }
}

/// Strategy when the history+memory plan would overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackStrategy {
    /// Drop the oldest history turns until the budget fits.
    TruncateOldest,
    /// Compress the oldest k turns into a single summary block.
    SummarizeOldestK(u32),
    /// Drop near-duplicate memories first (uses dedup_radius).
    DropDuplicates,
    /// No fallback; caller handles the overflow.
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallPlan {
    pub k: u32,
    /// Per-memory token budget. The recall path truncates each
    /// returned memory to fit.
    pub chunk_tokens: u32,
    /// Cosine-similarity threshold above which two recalled
    /// memories are considered near-duplicates and one is dropped.
    pub dedup_radius: f32,
    pub fallback: FallbackStrategy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub text: String,
    pub estimated_tokens: u32,
}

pub fn plan_recall(b: &ContextBudget, history_tokens: u32, query: &Query) -> RecallPlan {
    let avail = b.available();
    let mem_budget = b.memory_budget();

    // Sanity: history shouldn't be planned past the available
    // budget. If it is, kick in TruncateOldest as the fallback.
    let history_share = avail.saturating_sub(mem_budget);
    let fallback = if history_tokens > history_share {
        FallbackStrategy::TruncateOldest
    } else if mem_budget > 100_000 {
        // 1M-class window: aggressive dedup.
        FallbackStrategy::DropDuplicates
    } else {
        FallbackStrategy::None
    };

    // Per-memory chunk budget. We aim for ~1000 tokens per chunk on
    // 1M-class contexts, dropping to ~256 on 128k-class. Operators
    // can override by post-processing the plan.
    let chunk_tokens = if b.total_tokens >= 800_000 {
        1024
    } else if b.total_tokens >= 200_000 {
        512
    } else {
        256
    };

    // k: how many memories the planner asks for. Heuristic: spend
    // ~70% of mem_budget on memory bodies, the remaining 30% buffers
    // dedup + chunk overhead.
    let usable = (mem_budget as f32 * 0.7) as u32;
    let k = if chunk_tokens == 0 {
        0
    } else {
        (usable / chunk_tokens).clamp(1, 256)
    };

    // Lighter dedup on small windows (less risk of redundancy);
    // tighter on large.
    let dedup_radius = if b.total_tokens >= 800_000 {
        0.92
    } else {
        0.88
    };

    let _ = query; // reserved for future query-aware planning

    RecallPlan {
        k,
        chunk_tokens,
        dedup_radius,
        fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(t: u32) -> Query {
        Query {
            text: "test".into(),
            estimated_tokens: t,
        }
    }

    #[test]
    fn deepseek_v4_yields_high_k_and_fits_under_mem_share() {
        let b = ContextBudget::for_model(ModelId::DeepSeekV4_1m);
        let plan = plan_recall(&b, /* history */ 200_000, &q(64));
        assert!(
            plan.k >= 64,
            "expected k>=64 for 1M context, got {}",
            plan.k
        );
        let injected = plan.k * plan.chunk_tokens;
        assert!(
            injected as f32 <= b.memory_budget() as f32 * 0.8,
            "plan injects {injected} but mem_budget is {}",
            b.memory_budget()
        );
    }

    #[test]
    fn small_window_drops_to_smaller_chunks() {
        let b = ContextBudget::for_model(ModelId::DeepSeekV3_128k);
        let plan = plan_recall(&b, 8_000, &q(64));
        assert!(plan.chunk_tokens <= 512);
    }

    #[test]
    fn budget_does_not_overflow_total() {
        // Property test (deterministic, since the planner is pure):
        // for every model in the table, system + response + history +
        // injected memory must be <= total.
        for (m, _) in super::super::models::MODEL_TABLE {
            let b = ContextBudget::for_model(*m);
            let plan = plan_recall(&b, 0, &q(0));
            let injected = plan.k * plan.chunk_tokens;
            let total = b.system_reserve + b.response_reserve + injected;
            assert!(
                total <= b.total_tokens,
                "model {} overflows: total {} > {}",
                m.as_str(),
                total,
                b.total_tokens
            );
        }
    }

    #[test]
    fn truncate_oldest_kicks_in_when_history_overflows() {
        let b = ContextBudget::for_model(ModelId::Gpt5_1_128k);
        // History eats all available — should trigger fallback.
        let plan = plan_recall(&b, b.available() + 10_000, &q(1));
        assert_eq!(plan.fallback, FallbackStrategy::TruncateOldest);
    }

    #[test]
    fn dedup_radius_is_tighter_on_large_windows() {
        let small = plan_recall(&ContextBudget::for_model(ModelId::Gpt5_1_128k), 1000, &q(1));
        let huge = plan_recall(
            &ContextBudget::for_model(ModelId::Gemini2_5Pro2m),
            1000,
            &q(1),
        );
        assert!(huge.dedup_radius >= small.dedup_radius);
    }

    #[test]
    fn mem_share_is_clampable() {
        let b = ContextBudget::for_model(ModelId::Claude3_7Sonnet1m).with_mem_share(2.0);
        assert!(b.mem_share <= 1.0);
    }
}
