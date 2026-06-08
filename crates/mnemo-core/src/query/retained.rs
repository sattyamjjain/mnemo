//! Budgeted evidence retention for recall (EMBER, arXiv:2606.05894).
//!
//! The default recall path returns whole memory records. For an LLM
//! caller that must keep evidence *resident* in a bounded context
//! window, dumping raw chunks burns the budget fast: a handful of full
//! records and the window is full, even though most of each record is
//! filler around one salient fact.
//!
//! EMBER (*Efficient Memory By Evidence Retention*, arXiv:2606.05894)
//! frames this as a **writer** problem: under a fixed retained-token
//! budget, learn to keep compact, *recoverable* evidence rather than raw
//! text. This module is a **v0 stand-in** for that learned writer:
//!
//! - Instead of dumping raw chunks, it emits verbatim **evidence
//!   capsules** — a short verbatim excerpt of the record plus a
//!   **retrieval key** (the record id) so the caller can re-fetch the
//!   full chunk on demand. A capsule costs a fraction of the raw chunk,
//!   so many more distinct facts survive the same budget.
//! - Capsules are packed greedily under the budget, ranked by a simple
//!   **recoverability heuristic** — `recency × retrieval-hit-rate` —
//!   the v0 stand-in for EMBER's learned retention score. Recently
//!   written and frequently-retrieved records are the cheapest to keep
//!   resident because they are the most likely to be needed again.
//!
//! # Wiring
//!
//! [`RecallRequest::retained_token_budget`](crate::query::recall::RecallRequest::retained_token_budget)
//! carries the optional cap. When `Some(budget)`, the engine builds a
//! [`RetentionReport`] from the final recall result and returns it in
//! [`RecallResponse::retained_evidence`](crate::query::recall::RecallResponse::retained_evidence).
//! It is purely **additive**: the `memories` list is unchanged, so every
//! existing caller is unaffected; the capsule view is an extra,
//! opt-in projection of the same hits.
//!
//! # What this is NOT
//!
//! - **Not EMBER's learned writer.** The recoverability score is a
//!   hand-rolled `recency × hit-rate` heuristic, not a trained model.
//!   The arXiv:2606.05894 reference is a framing anchor, not a
//!   reproduction claim.
//! - **Not lossy on the store.** Capsules are a *read-side* projection;
//!   nothing is mutated or deleted. The retrieval key recovers the full
//!   record verbatim from the backend.
//! - **Not a tokenizer.** Token counts are the standard `ceil(chars/4)`
//!   rough-cut estimate; absolute counts are approximate, the
//!   budget-vs-naive *comparison* is the signal.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Recency half-life (hours) for the recoverability score. Matches the
/// engine's 1-week recency default elsewhere.
const RECENCY_HALF_LIFE_HOURS: f64 = 168.0;
/// Laplace-style smoothing for the hit-rate term so a never-accessed
/// (but freshly written) record still scores non-zero.
const HIT_RATE_SMOOTH: f64 = 5.0;
/// Default verbatim excerpt cap per capsule, in estimated tokens.
pub const DEFAULT_EXCERPT_TOKENS: usize = 64;
/// A capsule below this excerpt size carries too little verbatim
/// evidence to be worth a slot; once the remaining budget cannot fund
/// it, packing stops.
const MIN_EXCERPT_TOKENS: usize = 16;

/// Rough-cut token estimate: `ceil(chars / 4)`. There is no tokenizer
/// in the core, so this is a deliberate approximation (see the
/// module-level "what this is NOT").
pub fn est_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// Truncate `text` to at most `max_chars` characters on a char
/// boundary (never splits a multi-byte scalar).
fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// `recency × retrieval-hit-rate` — the v0 recoverability stand-in for
/// EMBER's learned retention score.
///
/// - **recency** decays with a 1-week half-life: `0.5^(age / 168h)`,
///   in `(0, 1]`.
/// - **hit-rate** is the smoothed access frequency
///   `(access + 1) / (access + 1 + 5)`, in `(0, 1)` — monotonically
///   increasing in `access_count`, and non-zero even at `access = 0`.
pub fn recoverability(age_hours: f64, access_count: u64) -> f32 {
    let recency = 0.5f64.powf(age_hours.max(0.0) / RECENCY_HALF_LIFE_HOURS);
    let ac = access_count as f64;
    let hit_rate = (ac + 1.0) / (ac + 1.0 + HIT_RATE_SMOOTH);
    (recency * hit_rate) as f32
}

/// One candidate record handed to [`retain_within_budget`].
#[derive(Debug, Clone)]
pub struct RetentionCandidate<'a> {
    /// Record id — becomes the capsule's retrieval key.
    pub id: Uuid,
    /// Verbatim record content.
    pub content: &'a str,
    /// How many times the record has been retrieved (hit-rate signal).
    pub access_count: u64,
    /// Age of the record in hours (recency signal).
    pub age_hours: f64,
    /// Fused retrieval score — tie-breaker only.
    pub retrieval_score: f32,
}

/// A verbatim evidence capsule: a short excerpt plus the retrieval key
/// that recovers the full record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceCapsule {
    /// Record id (also the retrieval key, parsed).
    pub id: Uuid,
    /// Retrieval key the caller uses to re-fetch the full record.
    pub retrieval_key: String,
    /// Verbatim prefix of the record content (never paraphrased).
    pub excerpt: String,
    /// Estimated tokens this capsule costs (excerpt + key).
    pub tokens: usize,
    /// The `recency × hit-rate` score this capsule was ranked by.
    pub recoverability: f32,
}

/// Diagnostics + capsules produced under a retained-token budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionReport {
    /// The cap the caller requested.
    pub budget_tokens: usize,
    /// Estimated tokens actually retained across all capsules.
    pub retained_tokens: usize,
    /// The packed capsules, in recoverability order.
    pub capsules: Vec<EvidenceCapsule>,
    /// How many candidate records were considered.
    pub candidates_examined: usize,
    /// Candidates that did not fit the budget.
    pub dropped: usize,
}

/// Pack the highest-recoverability evidence into `budget_tokens`,
/// preferring compact capsules over raw chunk dumps.
///
/// Candidates are ranked by [`recoverability`] (ties broken by
/// retrieval score, then input order) and greedily packed: each capsule
/// reserves its retrieval-key tokens and fills the rest with a verbatim
/// excerpt of up to `excerpt_tokens`. Packing stops once the remaining
/// budget cannot fund another minimally-useful capsule. The function is
/// pure — it never mutates or reorders the caller's records.
pub fn retain_within_budget(
    candidates: &[RetentionCandidate<'_>],
    budget_tokens: usize,
    excerpt_tokens: usize,
) -> RetentionReport {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&a, &b| {
        let ra = recoverability(candidates[a].age_hours, candidates[a].access_count);
        let rb = recoverability(candidates[b].age_hours, candidates[b].access_count);
        rb.partial_cmp(&ra)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                candidates[b]
                    .retrieval_score
                    .partial_cmp(&candidates[a].retrieval_score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.cmp(&b))
    });

    let mut remaining = budget_tokens;
    let mut capsules = Vec::new();
    let mut retained_tokens = 0usize;

    for &i in &order {
        let c = &candidates[i];
        let key = c.id.to_string();
        let key_tokens = est_tokens(&key);
        // All keys are uniform-length UUIDs, so once a minimally-useful
        // capsule no longer fits, none of the remaining ones will.
        if remaining < key_tokens + MIN_EXCERPT_TOKENS {
            break;
        }
        let excerpt_budget = (remaining - key_tokens).min(excerpt_tokens);
        let excerpt = truncate_chars(c.content, excerpt_budget * 4);
        let excerpt_cost = est_tokens(&excerpt);
        let cost = excerpt_cost + key_tokens;
        if cost > remaining {
            continue;
        }
        remaining -= cost;
        retained_tokens += cost;
        capsules.push(EvidenceCapsule {
            id: c.id,
            retrieval_key: key,
            excerpt,
            tokens: cost,
            recoverability: recoverability(c.age_hours, c.access_count),
        });
    }

    RetentionReport {
        budget_tokens,
        retained_tokens,
        dropped: candidates.len().saturating_sub(capsules.len()),
        candidates_examined: candidates.len(),
        capsules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand<'a>(id: Uuid, content: &'a str, access: u64, age_hours: f64) -> RetentionCandidate<'a> {
        RetentionCandidate {
            id,
            content,
            access_count: access,
            age_hours,
            retrieval_score: 1.0,
        }
    }

    #[test]
    fn est_tokens_is_ceil_div_four() {
        assert_eq!(est_tokens(""), 0);
        assert_eq!(est_tokens("abcd"), 1);
        assert_eq!(est_tokens("abcde"), 2);
    }

    #[test]
    fn recoverability_rewards_recent_and_frequent() {
        // Fresh + frequently hit beats stale + never hit.
        let fresh_hot = recoverability(1.0, 50);
        let stale_cold = recoverability(720.0, 0);
        assert!(fresh_hot > stale_cold);
        // Monotonic in access_count at fixed age.
        assert!(recoverability(10.0, 20) > recoverability(10.0, 1));
        // Monotonic (decreasing) in age at fixed hits.
        assert!(recoverability(1.0, 10) > recoverability(500.0, 10));
        // Non-zero even when never accessed.
        assert!(recoverability(0.0, 0) > 0.0);
    }

    #[test]
    fn packing_never_exceeds_budget() {
        let ids: Vec<Uuid> = (0..20).map(|_| Uuid::now_v7()).collect();
        let body = "the salient fact is alpha-bravo-charlie ".repeat(20);
        let cands: Vec<RetentionCandidate> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| cand(*id, &body, i as u64, i as f64))
            .collect();
        let report = retain_within_budget(&cands, 512, DEFAULT_EXCERPT_TOKENS);
        assert!(report.retained_tokens <= report.budget_tokens);
        assert_eq!(report.candidates_examined, 20);
        assert_eq!(report.capsules.len() + report.dropped, 20);
        // Capsules are emitted in non-increasing recoverability order.
        for w in report.capsules.windows(2) {
            assert!(w[0].recoverability >= w[1].recoverability);
        }
    }

    #[test]
    fn capsules_beat_raw_dumps_on_count_under_budget() {
        // Each record is a large raw chunk; the salient fact is at the
        // front. Capsules keep only the front excerpt + key, so far more
        // distinct records survive the same budget than raw dumps do.
        let ids: Vec<Uuid> = (0..40).map(|_| Uuid::now_v7()).collect();
        let chunk = format!("FACT {} ", "x".repeat(600));
        let cands: Vec<RetentionCandidate> =
            ids.iter().map(|id| cand(*id, &chunk, 1, 1.0)).collect();
        let budget = 2048;
        let report = retain_within_budget(&cands, budget, DEFAULT_EXCERPT_TOKENS);

        // Naive truncation: pack whole raw chunks until the budget runs
        // out. chunk ~= est_tokens(600+ chars) ~= 152 tokens.
        let chunk_tokens = est_tokens(&chunk);
        let naive_fit = budget / chunk_tokens;
        assert!(
            report.capsules.len() > naive_fit,
            "capsules {} should beat naive {} under {budget} tokens",
            report.capsules.len(),
            naive_fit
        );
        // And every retained capsule still carries the verbatim "FACT"
        // prefix (recoverable evidence, not a paraphrase).
        assert!(
            report
                .capsules
                .iter()
                .all(|c| c.excerpt.starts_with("FACT"))
        );
    }

    #[test]
    fn zero_budget_keeps_nothing() {
        let id = Uuid::now_v7();
        let report =
            retain_within_budget(&[cand(id, "anything", 1, 1.0)], 0, DEFAULT_EXCERPT_TOKENS);
        assert!(report.capsules.is_empty());
        assert_eq!(report.dropped, 1);
        assert_eq!(report.retained_tokens, 0);
    }
}
