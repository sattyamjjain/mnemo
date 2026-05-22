//! v0.4.7 — Current-fact resolver: an opt-in post-processor over the
//! standard `recall` result set.
//!
//! # Anchor
//!
//! [arXiv:2605.18565](https://arxiv.org/abs/2605.18565) (MINTEval —
//! Memory Interference under Targeted Edits) measures how often
//! memory systems return a *superseded* value of a fact after the
//! same fact has been revised K times. The default mnemo recall
//! path (semantic + BM25 + graph + recency) is unaware of fact
//! identity — every write is a separate `MemoryRecord`. Under
//! interference, the recency lane alone is often not enough: high-
//! semantic-score older versions can still outrank the most recent
//! write.
//!
//! The current-fact resolver runs *after* the normal recall result
//! set is computed. Given a `fact_key` (a metadata JSON pointer the
//! operator chose to scope fact identity by — typical convention
//! is `"fact_id"`), the resolver groups candidates by their value
//! under that key, picks the most-recent write per group, and
//! optionally returns the older writes in that group as a
//! supersession chain.
//!
//! # Design contract
//!
//! - **Opt-in.** Triggered only when
//!   [`RecallRequest::current_fact_resolver`][crate::query::recall::RecallRequest::current_fact_resolver]
//!   is `Some`. The default read path is unchanged.
//! - **Post-processor, not a replacement.** The resolver runs over
//!   whatever candidates the underlying `RetrievalMode` produced.
//!   It does not re-issue a query.
//! - **Most-recent wins.** Within each fact-identity group, the
//!   record with the latest `updated_at` (falling back to
//!   `created_at`) is the *current* fact. If two records share the
//!   same timestamp, the higher base score wins; if scores tie,
//!   the higher UUID v7 wins (which is itself time-sortable).
//! - **Supersession chain.** Older writes in a group are returned
//!   in
//!   [`RecallResponse::superseded`][crate::query::recall::RecallResponse::superseded]
//!   when [`CurrentFactResolverConfig::include_supersession_chain`]
//!   is `true`. Chain is ordered newest-superseded → oldest.
//! - **Records without the `fact_key`** stay in the result set
//!   untouched (they have no fact identity to resolve against).
//!
//! # What this module is NOT
//!
//! - **Not a contradiction detector.** Two records with the same
//!   `fact_key` value are treated as versions of one fact; the
//!   resolver does NOT inspect content to detect semantic
//!   contradiction. The operator chooses `fact_key` to mean what
//!   they want it to mean.
//! - **Not a write-side guard.** The resolver only re-ranks reads.
//!   Operators wanting to *prevent* contradictory writes use the
//!   existing conflict-resolution path (`crate::query::conflict`).
//! - **Not a benchmark.** The MINTEval-shaped interference scenario
//!   that exercises this resolver lives in
//!   [`bench/locomo/src/bin/interference.rs`](../../../../bench/locomo/src/bin/interference.rs).

use serde::{Deserialize, Serialize};

use crate::query::recall::{ScoredMemory, SupersededRecord};

/// Opt-in config for the current-fact resolver. Carried on
/// [`RecallRequest::current_fact_resolver`][crate::query::recall::RecallRequest::current_fact_resolver].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentFactResolverConfig {
    /// JSON metadata key the resolver groups candidates by. Convention
    /// is `"fact_id"`; the operator chooses a key that fits their
    /// fact-identity scheme. Records missing this key are passed
    /// through untouched.
    pub fact_key: String,
    /// When `true`, the response carries the older records as a
    /// supersession chain in
    /// [`RecallResponse::superseded`][crate::query::recall::RecallResponse::superseded].
    /// When `false` (default), older records are silently dropped.
    #[serde(default)]
    pub include_supersession_chain: bool,
}

impl CurrentFactResolverConfig {
    pub fn new<S: Into<String>>(fact_key: S) -> Self {
        Self {
            fact_key: fact_key.into(),
            include_supersession_chain: false,
        }
    }
    pub fn with_supersession_chain(mut self) -> Self {
        self.include_supersession_chain = true;
        self
    }
}

/// Output of the resolver: the kept (current) candidates + (when
/// requested) the superseded ones grouped by fact identity.
#[derive(Debug, Clone)]
pub struct ResolverOutput {
    pub kept: Vec<ScoredMemory>,
    pub superseded: Vec<SupersededRecord>,
}

/// Apply the current-fact resolver to a result set produced by the
/// standard recall path. Groups by `cfg.fact_key`, keeps the most
/// recent write per group, and optionally collects the older
/// writes as a supersession chain.
///
/// Records missing the `fact_key` field in their metadata are
/// passed through to `kept` untouched.
pub fn resolve(cfg: &CurrentFactResolverConfig, candidates: Vec<ScoredMemory>) -> ResolverOutput {
    use std::collections::BTreeMap;

    let mut without_key: Vec<ScoredMemory> = Vec::new();
    let mut groups: BTreeMap<String, Vec<ScoredMemory>> = BTreeMap::new();

    for cand in candidates {
        match extract_fact_id(&cand, &cfg.fact_key) {
            Some(id) => groups.entry(id).or_default().push(cand),
            None => without_key.push(cand),
        }
    }

    let mut kept: Vec<ScoredMemory> = without_key;
    let mut superseded: Vec<SupersededRecord> = Vec::new();

    for (fact_id, mut group) in groups {
        // Sort newest → oldest. Tie-break by score (higher wins),
        // then by UUID v7 (which is itself time-sortable).
        group.sort_by(|a, b| {
            cmp_recency_desc(a, b)
                .then_with(|| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| b.id.cmp(&a.id))
        });

        let mut it = group.into_iter();
        if let Some(current) = it.next() {
            if cfg.include_supersession_chain {
                let current_id = current.id;
                let current_updated_at = current.updated_at.clone();
                for older in it {
                    superseded.push(SupersededRecord {
                        id: older.id,
                        fact_id: fact_id.clone(),
                        superseded_by: current_id,
                        superseded_at: current_updated_at.clone(),
                        prior_updated_at: older.updated_at.clone(),
                    });
                }
            }
            kept.push(current);
        }
    }

    // Re-sort kept by the original-style score-desc so the response
    // shape stays consistent with the non-resolver path.
    kept.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    ResolverOutput { kept, superseded }
}

fn extract_fact_id(m: &ScoredMemory, fact_key: &str) -> Option<String> {
    let v = m.metadata.get(fact_key)?;
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if let Some(n) = v.as_i64() {
        return Some(n.to_string());
    }
    if let Some(b) = v.as_bool() {
        return Some(b.to_string());
    }
    None
}

fn cmp_recency_desc(a: &ScoredMemory, b: &ScoredMemory) -> std::cmp::Ordering {
    let a_t = effective_timestamp(a);
    let b_t = effective_timestamp(b);
    b_t.cmp(a_t)
}

fn effective_timestamp(m: &ScoredMemory) -> &str {
    if m.updated_at.is_empty() {
        m.created_at.as_str()
    } else {
        m.updated_at.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::memory::{MemoryType, Scope};
    use serde_json::json;
    use uuid::Uuid;

    fn hit(fact: Option<&str>, updated_at: &str, score: f32, content: &str) -> ScoredMemory {
        ScoredMemory {
            id: Uuid::now_v7(),
            content: content.to_string(),
            agent_id: "test".to_string(),
            memory_type: MemoryType::Episodic,
            scope: Scope::Private,
            importance: 0.5,
            tags: vec![],
            metadata: match fact {
                Some(f) => json!({ "fact_id": f }),
                None => json!({}),
            },
            score,
            access_count: 0,
            created_at: updated_at.to_string(),
            updated_at: updated_at.to_string(),
            score_breakdown: None,
        }
    }

    #[test]
    fn keeps_most_recent_per_fact_group() {
        let cfg = CurrentFactResolverConfig::new("fact_id");
        let out = resolve(
            &cfg,
            vec![
                hit(Some("f1"), "2026-05-22T08:00:00Z", 0.9, "old v1"),
                hit(Some("f1"), "2026-05-22T09:00:00Z", 0.5, "new v2"),
                hit(Some("f1"), "2026-05-22T07:00:00Z", 0.7, "older v0"),
            ],
        );
        assert_eq!(out.kept.len(), 1);
        assert_eq!(out.kept[0].content, "new v2");
        assert!(out.superseded.is_empty(), "chain off by default");
    }

    #[test]
    fn supersession_chain_when_enabled() {
        let cfg = CurrentFactResolverConfig::new("fact_id").with_supersession_chain();
        let out = resolve(
            &cfg,
            vec![
                hit(Some("f1"), "2026-05-22T08:00:00Z", 0.9, "v1"),
                hit(Some("f1"), "2026-05-22T09:00:00Z", 0.5, "v2 current"),
                hit(Some("f1"), "2026-05-22T07:00:00Z", 0.7, "v0 oldest"),
            ],
        );
        assert_eq!(out.kept.len(), 1);
        assert_eq!(out.kept[0].content, "v2 current");
        assert_eq!(out.superseded.len(), 2);
        // Chain ordered newest-superseded → oldest.
        assert_eq!(out.superseded[0].prior_updated_at, "2026-05-22T08:00:00Z");
        assert_eq!(out.superseded[1].prior_updated_at, "2026-05-22T07:00:00Z");
        for s in &out.superseded {
            assert_eq!(s.fact_id, "f1");
            assert_eq!(s.superseded_by, out.kept[0].id);
        }
    }

    #[test]
    fn records_without_fact_key_pass_through() {
        let cfg = CurrentFactResolverConfig::new("fact_id");
        let out = resolve(
            &cfg,
            vec![
                hit(None, "2026-05-22T08:00:00Z", 0.9, "no-fact-a"),
                hit(None, "2026-05-22T09:00:00Z", 0.4, "no-fact-b"),
                hit(Some("f1"), "2026-05-22T07:00:00Z", 0.7, "fact"),
            ],
        );
        // All three kept; the two without fact_id pass through, the
        // single fact-id group has one member.
        assert_eq!(out.kept.len(), 3);
        let contents: Vec<_> = out.kept.iter().map(|m| m.content.as_str()).collect();
        assert!(contents.contains(&"no-fact-a"));
        assert!(contents.contains(&"no-fact-b"));
        assert!(contents.contains(&"fact"));
    }

    #[test]
    fn multi_group_resolution() {
        let cfg = CurrentFactResolverConfig::new("fact_id").with_supersession_chain();
        let out = resolve(
            &cfg,
            vec![
                hit(Some("city"), "2026-05-22T08:00:00Z", 0.9, "Paris"),
                hit(Some("city"), "2026-05-22T09:00:00Z", 0.5, "Berlin"),
                hit(Some("color"), "2026-05-22T07:00:00Z", 0.7, "red"),
                hit(Some("color"), "2026-05-22T08:30:00Z", 0.4, "blue"),
            ],
        );
        assert_eq!(out.kept.len(), 2);
        assert_eq!(out.superseded.len(), 2);
        let kept_contents: Vec<_> = out.kept.iter().map(|m| m.content.as_str()).collect();
        assert!(kept_contents.contains(&"Berlin"));
        assert!(kept_contents.contains(&"blue"));
    }

    #[test]
    fn empty_candidate_set_returns_empty_output() {
        let cfg = CurrentFactResolverConfig::new("fact_id");
        let out = resolve(&cfg, vec![]);
        assert!(out.kept.is_empty());
        assert!(out.superseded.is_empty());
    }

    #[test]
    fn fact_id_can_be_integer() {
        let cfg = CurrentFactResolverConfig::new("fact_id");
        let mut newer = hit(None, "2026-05-22T09:00:00Z", 0.5, "newer");
        newer.metadata = json!({"fact_id": 42});
        let mut older = hit(None, "2026-05-22T08:00:00Z", 0.9, "older");
        older.metadata = json!({"fact_id": 42});
        let out = resolve(&cfg, vec![older, newer]);
        assert_eq!(out.kept.len(), 1);
        assert_eq!(out.kept[0].content, "newer");
    }
}
