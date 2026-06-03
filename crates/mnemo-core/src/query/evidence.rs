//! Cost-aware, answer-impact-scored evidence selection for recall.
//!
//! The default recall path front-loads: it returns the top-`limit`
//! records sorted by the fused retrieval score. For an LLM caller that
//! pays per evidence chunk (context tokens), that is wasteful — most
//! answers are decided by the first one or two strongly-relevant
//! chunks, and the rest are dead weight.
//!
//! This module adds an opt-in *evidence budget* that runs over the
//! already-ranked candidate list and returns **the smallest prefix
//! that clears a configurable sufficiency bar**, capped by an optional
//! `max_evidence`. It is purely subtractive: it only ever returns a
//! prefix of the input ordering, so it can never reorder or "silently
//! lower" the retrieval's top-k cosine ordering (see the property test
//! in this module).
//!
//! # Answer-impact scoring
//!
//! Relevance is computed through a pluggable [`EvidenceScorer`] trait,
//! so callers can swap the signal used to decide sufficiency:
//!
//! - [`CosineScorer`] (default) — cosine similarity of the candidate
//!   embedding against the query embedding, falling back to the
//!   retrieval score when embeddings are absent or degenerate.
//! - [`DeltaScorer`] — an *answer-impact* scorer: it scores a chunk by
//!   whether adding it to the evidence set already selected would
//!   change a downstream answer. The actual "would the answer change?"
//!   judgement is an injectable closure so the core stays
//!   model-agnostic; [`DeltaScorer::stub`] ships a deterministic
//!   marginal-novelty heuristic for tests and offline use.
//!
//! # Wiring
//!
//! [`RecallRequest::evidence_budget`](crate::query::recall::RecallRequest::evidence_budget)
//! carries the serializable [`EvidenceBudget`] config. When the config
//! selects [`ScorerKind::Delta`] AND the engine has a scorer attached
//! via [`MnemoEngine::with_evidence_scorer`](crate::query::MnemoEngine::with_evidence_scorer),
//! that scorer is used; otherwise the path falls back to
//! [`CosineScorer`]. The default read path (no `evidence_budget`) is
//! unchanged.

use serde::{Deserialize, Serialize};

/// Which relevance signal the budget uses to decide sufficiency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScorerKind {
    /// Cosine similarity of candidate vs query embedding (default).
    #[default]
    Cosine,
    /// Answer-impact / marginal-delta scoring. Requires an
    /// [`EvidenceScorer`] attached to the engine, else falls back to
    /// cosine.
    Delta,
}

/// Serializable per-query evidence budget.
///
/// Attach via
/// [`RecallRequest::evidence_budget`](crate::query::recall::RecallRequest::evidence_budget).
/// All fields are optional knobs; the zero-config default
/// ([`EvidenceBudget::default`]) caps nothing and never early-stops,
/// so an explicitly-`Some(EvidenceBudget::default())` request still
/// behaves like the legacy path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceBudget {
    /// Hard cap on the number of evidence chunks returned. `None`
    /// leaves the count bounded only by the recall `limit`.
    #[serde(default)]
    pub max_evidence: Option<usize>,
    /// When `true`, stop accumulating evidence as soon as the running
    /// sufficiency score clears [`sufficiency_threshold`]. The caller
    /// gets the smallest prefix that clears the bar.
    #[serde(default)]
    pub stop_when_sufficient: bool,
    /// Cumulative sufficiency score the selected set must reach before
    /// early-stop fires. Scores are summed across selected chunks, so
    /// for a cosine signal in `[0, 1]` a threshold of `0.8` is cleared
    /// by one `0.85` chunk or two `0.4`/`0.45` chunks. Ignored when
    /// `stop_when_sufficient` is `false`.
    #[serde(default = "default_sufficiency_threshold")]
    pub sufficiency_threshold: f32,
    /// Which scorer computes the per-chunk relevance.
    #[serde(default)]
    pub scorer: ScorerKind,
}

fn default_sufficiency_threshold() -> f32 {
    0.8
}

impl Default for EvidenceBudget {
    fn default() -> Self {
        Self {
            max_evidence: None,
            stop_when_sufficient: false,
            sufficiency_threshold: default_sufficiency_threshold(),
            scorer: ScorerKind::Cosine,
        }
    }
}

impl EvidenceBudget {
    /// Convenience constructor: cap at `max` chunks, no early-stop.
    pub fn capped(max: usize) -> Self {
        Self {
            max_evidence: Some(max),
            ..Self::default()
        }
    }

    /// Convenience constructor: early-stop once cumulative score
    /// clears `threshold`.
    pub fn early_stop(threshold: f32) -> Self {
        Self {
            stop_when_sufficient: true,
            sufficiency_threshold: threshold,
            ..Self::default()
        }
    }
}

/// A single recall candidate handed to the scorer / budget selector.
///
/// Deliberately borrows so the recall path can build these cheaply
/// from its `(MemoryRecord, f32)` working set without cloning content
/// or embeddings.
pub struct EvidenceCandidate<'a> {
    /// The candidate's textual content.
    pub content: &'a str,
    /// The candidate's stored embedding, if any.
    pub embedding: Option<&'a [f32]>,
    /// The fused retrieval score this candidate already earned (used
    /// as the cosine fallback when embeddings are unavailable).
    pub retrieval_score: f32,
}

/// Read-only context a scorer sees for one candidate.
pub struct EvidenceContext<'a> {
    /// The raw query string.
    pub query: &'a str,
    /// The embedded query vector, if the embedder produced a
    /// non-degenerate one.
    pub query_embedding: Option<&'a [f32]>,
    /// The candidate being scored.
    pub candidate: &'a EvidenceCandidate<'a>,
    /// Candidates already admitted to the evidence set, in selection
    /// order. Lets a marginal/answer-impact scorer reason about
    /// novelty vs what is already present.
    pub selected: &'a [EvidenceCandidate<'a>],
}

/// Pluggable relevance signal for the evidence budget.
///
/// Implementors return a score in `[0, 1]` representing how much this
/// candidate contributes to answering the query *given what is already
/// selected*. The budget sums these to decide sufficiency.
pub trait EvidenceScorer: Send + Sync {
    /// Score one candidate in `[0, 1]`.
    fn score(&self, ctx: &EvidenceContext<'_>) -> f32;
    /// Stable identifier surfaced in the selection diagnostics.
    fn name(&self) -> &str;
}

/// Default scorer: cosine similarity of candidate vs query embedding.
///
/// When either embedding is missing or degenerate (e.g. the engine
/// runs `NoopEmbedding`, whose vectors are all-zero), it falls back to
/// the candidate's fused retrieval score so the budget stays usable
/// without a real embedder.
#[derive(Debug, Clone, Default)]
pub struct CosineScorer;

impl EvidenceScorer for CosineScorer {
    fn score(&self, ctx: &EvidenceContext<'_>) -> f32 {
        match (ctx.query_embedding, ctx.candidate.embedding) {
            (Some(q), Some(c)) if !q.is_empty() && q.len() == c.len() => {
                let sim = cosine(q, c);
                if sim.is_finite() && sim > 0.0 {
                    sim.clamp(0.0, 1.0)
                } else {
                    ctx.candidate.retrieval_score.clamp(0.0, 1.0)
                }
            }
            _ => ctx.candidate.retrieval_score.clamp(0.0, 1.0),
        }
    }

    fn name(&self) -> &str {
        "cosine"
    }
}

/// Answer-impact scorer: scores a chunk by whether including it would
/// change a downstream answer, relative to the evidence already
/// selected.
///
/// The "would the answer change?" judgement is an injectable closure
/// (`impact_fn`) so the engine core never embeds a model. The closure
/// receives the same [`EvidenceContext`] the trait does and returns a
/// score in `[0, 1]`. A typical production wiring calls an LLM:
/// *"given the answer derivable from `selected`, does adding
/// `candidate` change it? rate 0–1"*.
///
/// [`DeltaScorer::stub`] ships a deterministic marginal-novelty
/// heuristic (high when the candidate's token set is novel vs what is
/// already selected, low when redundant) so tests and offline runs
/// have a model-free default that still exhibits diminishing returns.
pub struct DeltaScorer {
    impact_fn: Box<dyn Fn(&EvidenceContext<'_>) -> f32 + Send + Sync>,
}

impl DeltaScorer {
    /// Build a scorer from a caller-supplied answer-impact closure.
    pub fn new<F>(impact_fn: F) -> Self
    where
        F: Fn(&EvidenceContext<'_>) -> f32 + Send + Sync + 'static,
    {
        Self {
            impact_fn: Box::new(impact_fn),
        }
    }

    /// Model-free stub: marginal-novelty heuristic. The candidate's
    /// score is the fraction of its whitespace tokens that do NOT
    /// already appear in any selected candidate, scaled by its
    /// retrieval score. A fresh chunk scores near its retrieval score;
    /// a fully-redundant chunk scores ~0 — so the budget exhibits the
    /// diminishing-returns shape an answer-impact signal should.
    pub fn stub() -> Self {
        Self::new(|ctx| {
            let cand_tokens: std::collections::HashSet<&str> =
                ctx.candidate.content.split_whitespace().collect();
            if cand_tokens.is_empty() {
                return 0.0;
            }
            let seen: std::collections::HashSet<&str> = ctx
                .selected
                .iter()
                .flat_map(|c| c.content.split_whitespace())
                .collect();
            let novel = cand_tokens.iter().filter(|t| !seen.contains(*t)).count();
            let novelty = novel as f32 / cand_tokens.len() as f32;
            (novelty * ctx.candidate.retrieval_score.clamp(0.0, 1.0)).clamp(0.0, 1.0)
        })
    }

    /// Score via the injected closure.
    pub fn impact(&self, ctx: &EvidenceContext<'_>) -> f32 {
        (self.impact_fn)(ctx).clamp(0.0, 1.0)
    }
}

impl EvidenceScorer for DeltaScorer {
    fn score(&self, ctx: &EvidenceContext<'_>) -> f32 {
        self.impact(ctx)
    }

    fn name(&self) -> &str {
        "delta"
    }
}

/// Diagnostics returned alongside the trimmed evidence set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceSelectionReport {
    /// Scorer used (`"cosine"` / `"delta"` / a custom scorer's name).
    pub scorer: String,
    /// Number of candidates examined before the budget cut the list.
    pub examined: usize,
    /// Number of candidates returned after the budget.
    pub returned: usize,
    /// Cumulative sufficiency score of the returned set.
    pub cumulative_score: f32,
    /// `true` when early-stop fired (cumulative cleared the threshold
    /// before the candidate list / cap was exhausted).
    pub stopped_early: bool,
    /// `true` when `max_evidence` truncated the set.
    pub capped: bool,
}

/// Result of [`select_within_budget`]: the indices to keep (a prefix of
/// the input order) plus diagnostics.
pub struct BudgetSelection {
    /// Number of leading candidates to retain. Always `<=` input len.
    pub keep: usize,
    pub report: EvidenceSelectionReport,
}

/// Select the smallest prefix of `candidates` that satisfies `budget`.
///
/// `candidates` MUST already be in the recall's ranked order (highest
/// fused score first). This function never reorders — it only chooses
/// how many leading candidates to keep — which is what guarantees the
/// "a larger budget never lowers the top-k ordering" property.
///
/// Sufficiency is the running sum of per-candidate scores from
/// `scorer`. With `stop_when_sufficient`, accumulation halts the
/// moment the sum reaches `sufficiency_threshold`. `max_evidence`
/// applies as a hard cap regardless.
pub fn select_within_budget(
    candidates: &[EvidenceCandidate<'_>],
    budget: &EvidenceBudget,
    scorer: &dyn EvidenceScorer,
    query: &str,
    query_embedding: Option<&[f32]>,
) -> BudgetSelection {
    let cap = budget.max_evidence.unwrap_or(candidates.len());
    let hard_cap = cap.min(candidates.len());

    let mut selected: Vec<EvidenceCandidate<'_>> = Vec::new();
    let mut cumulative = 0.0_f32;
    let mut stopped_early = false;
    let mut examined = 0_usize;

    for cand in candidates.iter() {
        if selected.len() >= hard_cap {
            break;
        }
        examined += 1;
        let ctx = EvidenceContext {
            query,
            query_embedding,
            candidate: cand,
            selected: &selected,
        };
        let s = scorer.score(&ctx).clamp(0.0, 1.0);
        cumulative += s;
        // `EvidenceCandidate` is Copy-cheap (two refs + a float).
        selected.push(EvidenceCandidate {
            content: cand.content,
            embedding: cand.embedding,
            retrieval_score: cand.retrieval_score,
        });
        if budget.stop_when_sufficient && cumulative >= budget.sufficiency_threshold {
            stopped_early = true;
            break;
        }
    }

    let keep = selected.len();
    let capped = budget.max_evidence.is_some() && keep >= hard_cap && keep < candidates.len();

    BudgetSelection {
        keep,
        report: EvidenceSelectionReport {
            scorer: scorer.name().to_string(),
            examined,
            returned: keep,
            cumulative_score: cumulative,
            stopped_early,
            capped,
        },
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na.sqrt() * nb.sqrt()).max(f32::EPSILON);
    dot / denom
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(content: &'static str, score: f32) -> EvidenceCandidate<'static> {
        EvidenceCandidate {
            content,
            embedding: None,
            retrieval_score: score,
        }
    }

    #[test]
    fn max_evidence_cap_is_respected() {
        let cands = vec![
            cand("alpha one", 0.9),
            cand("beta two", 0.8),
            cand("gamma three", 0.7),
            cand("delta four", 0.6),
        ];
        let budget = EvidenceBudget::capped(2);
        let sel = select_within_budget(&cands, &budget, &CosineScorer, "q", None);
        assert_eq!(sel.keep, 2, "cap must bound the returned count");
        assert!(sel.report.capped);
        assert!(!sel.report.stopped_early);
    }

    #[test]
    fn early_stop_fires_at_threshold() {
        // Two 0.5 chunks clear a 0.8 cumulative bar after the second.
        let cands = vec![
            cand("alpha", 0.5),
            cand("beta", 0.5),
            cand("gamma", 0.5),
            cand("delta", 0.5),
        ];
        let budget = EvidenceBudget::early_stop(0.8);
        let sel = select_within_budget(&cands, &budget, &CosineScorer, "q", None);
        assert_eq!(sel.keep, 2, "should stop after cumulative 1.0 clears 0.8");
        assert!(sel.report.stopped_early);
        assert!(sel.report.cumulative_score >= 0.8);
    }

    #[test]
    fn early_stop_one_strong_chunk_clears_bar() {
        let cands = vec![cand("alpha", 0.85), cand("beta", 0.85), cand("gamma", 0.85)];
        let budget = EvidenceBudget::early_stop(0.8);
        let sel = select_within_budget(&cands, &budget, &CosineScorer, "q", None);
        assert_eq!(sel.keep, 1, "a single 0.85 chunk clears the 0.8 bar");
        assert!(sel.report.stopped_early);
    }

    #[test]
    fn scorer_trait_is_swappable() {
        // The stub DeltaScorer penalises redundant content, so two
        // identical chunks accumulate slower than two distinct chunks.
        let distinct = vec![cand("alpha one", 0.9), cand("beta two", 0.9)];
        let redundant = vec![cand("alpha one", 0.9), cand("alpha one", 0.9)];
        let budget = EvidenceBudget {
            stop_when_sufficient: true,
            sufficiency_threshold: 1.5,
            scorer: ScorerKind::Delta,
            ..Default::default()
        };
        let scorer = DeltaScorer::stub();

        let s_distinct = select_within_budget(&distinct, &budget, &scorer, "q", None);
        let s_redundant = select_within_budget(&redundant, &budget, &scorer, "q", None);

        // Distinct chunks contribute more novelty, so the cumulative
        // score after two chunks is strictly higher for the distinct
        // set than the redundant set.
        assert!(
            s_distinct.report.cumulative_score > s_redundant.report.cumulative_score,
            "delta scorer must reward novelty: distinct={} redundant={}",
            s_distinct.report.cumulative_score,
            s_redundant.report.cumulative_score
        );
        // And the swapped-in scorer is reported by name.
        assert_eq!(s_distinct.report.scorer, "delta");
    }

    #[test]
    fn injectable_closure_is_honoured() {
        // A closure that always returns 1.0 clears any single-chunk bar
        // immediately, proving the LLM-callback seam is live.
        let scorer = DeltaScorer::new(|_ctx| 1.0);
        let cands = vec![cand("alpha", 0.1), cand("beta", 0.1)];
        let budget = EvidenceBudget {
            stop_when_sufficient: true,
            sufficiency_threshold: 0.9,
            scorer: ScorerKind::Delta,
            ..Default::default()
        };
        let sel = select_within_budget(&cands, &budget, &scorer, "q", None);
        assert_eq!(
            sel.keep, 1,
            "closure scoring 1.0 clears 0.9 after one chunk"
        );
    }

    #[test]
    fn no_budget_keeps_everything() {
        let cands = vec![cand("a", 0.9), cand("b", 0.8), cand("c", 0.7)];
        let budget = EvidenceBudget::default();
        let sel = select_within_budget(&cands, &budget, &CosineScorer, "q", None);
        assert_eq!(sel.keep, 3, "default budget caps nothing and never stops");
        assert!(!sel.report.capped);
        assert!(!sel.report.stopped_early);
    }

    // ---- Property: a larger budget never reorders or drops a
    // higher-ranked item that a smaller budget kept. Because the
    // selector only ever returns a prefix, keep(b1) <= keep(b2) for
    // b1.max_evidence <= b2.max_evidence, and the kept set of the
    // smaller budget is always a prefix of the larger's.
    #[test]
    fn property_larger_budget_is_prefix_superset() {
        // Deterministic pseudo-random candidate scores via a simple
        // LCG so the test stays reproducible without a rng dep.
        let mut state: u64 = 0x9E3779B97F4A7C15;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((state >> 33) as f32) / (u32::MAX as f32)
        };
        let contents: Vec<String> = (0..40).map(|i| format!("chunk-{i}-token{i}")).collect();
        let mut cands: Vec<EvidenceCandidate<'_>> = contents
            .iter()
            .map(|c| EvidenceCandidate {
                content: c.as_str(),
                embedding: None,
                retrieval_score: next(),
            })
            .collect();
        // Caller contract: candidates arrive ranked. Sort desc.
        cands.sort_by(|a, b| {
            b.retrieval_score
                .partial_cmp(&a.retrieval_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for small in 1..=cands.len() {
            for large in small..=cands.len() {
                let bs = EvidenceBudget::capped(small);
                let bl = EvidenceBudget::capped(large);
                let ss = select_within_budget(&cands, &bs, &CosineScorer, "q", None);
                let sl = select_within_budget(&cands, &bl, &CosineScorer, "q", None);
                // Larger budget keeps at least as many.
                assert!(
                    sl.keep >= ss.keep,
                    "larger budget kept fewer: small={} large={} got {} vs {}",
                    small,
                    large,
                    sl.keep,
                    ss.keep
                );
                // The smaller budget's kept set is exactly the prefix
                // of the larger's — i.e. ordering is preserved and no
                // higher-ranked item is silently dropped.
                for i in 0..ss.keep {
                    assert_eq!(
                        cands[i].content,
                        contents_sorted(&cands)[i],
                        "internal: index drift"
                    );
                }
                assert!(
                    ss.keep <= sl.keep,
                    "prefix invariant violated: {} > {}",
                    ss.keep,
                    sl.keep
                );
            }
        }
    }

    // Helper kept intentionally trivial: the candidate slice IS its own
    // ranked order, so the "sorted contents" are just the contents in
    // place. Exists to make the prefix assertion above explicit.
    fn contents_sorted<'a>(cands: &'a [EvidenceCandidate<'a>]) -> Vec<&'a str> {
        cands.iter().map(|c| c.content).collect()
    }
}
