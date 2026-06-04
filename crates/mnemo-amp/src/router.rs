//! Fan-out router + cross-adapter fusion primitives.
//!
//! [`AmpRouter`] is the single entry point a transport hands an
//! [`AmpEnvelope`] to: it dispatches to the backing [`MemoryStore`].
//! When more than one store is registered it **fans out** writes to
//! all of them and fuses reads, which is the shape the AMP
//! cross-adapter conformance suite exercises (one logical memory
//! surface backed by several adapters).
//!
//! The fusion lane ships two combiners — [`rrf_fuse`] (Reciprocal Rank
//! Fusion) and [`max_fuse`] (max-score) — used by the conformance test
//! to demonstrate RRF's robustness to a rank-0 adversarial injection
//! that max-fusion is fooled by.

use std::sync::Arc;

use crate::error::AmpError;
use crate::store::MemoryStore;
use crate::wire::{AmpEnvelope, AmpHit, AmpOp, AmpResult};

/// Routes AMP envelopes to one or more [`MemoryStore`] backends.
#[derive(Clone)]
pub struct AmpRouter {
    stores: Vec<Arc<dyn MemoryStore>>,
    /// RRF rank constant `k`. 60 is the canonical TREC default.
    rrf_k: f32,
}

impl AmpRouter {
    /// Single-backend router (the common case).
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self {
            stores: vec![store],
            rrf_k: 60.0,
        }
    }

    /// Fan-out router over several backends.
    pub fn fan_out(stores: Vec<Arc<dyn MemoryStore>>) -> Self {
        Self {
            stores,
            rrf_k: 60.0,
        }
    }

    pub fn with_rrf_k(mut self, k: f32) -> Self {
        self.rrf_k = k;
        self
    }

    /// Route one envelope. Writes (`remember` / `forget` / `merge` /
    /// `expire`) fan out to every backend; the first backend's result
    /// is returned, with any backend error surfaced. `recall` fans out
    /// and fuses the per-backend hit lists with RRF.
    pub async fn route(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError> {
        match env.op {
            AmpOp::Recall => self.route_recall(env).await,
            _ => {
                let mut first: Option<AmpResult> = None;
                for store in &self.stores {
                    let r = store.dispatch(env).await?;
                    if first.is_none() {
                        first = Some(r);
                    }
                }
                first.ok_or_else(|| AmpError::Validation("router has no backends".into()))
            }
        }
    }

    async fn route_recall(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError> {
        if self.stores.len() == 1 {
            return self.stores[0].recall(env).await;
        }
        let mut lists: Vec<Vec<AmpHit>> = Vec::with_capacity(self.stores.len());
        for store in &self.stores {
            lists.push(store.recall(env).await?.hits);
        }
        let fused = rrf_fuse(&lists, self.rrf_k);
        let mut out = AmpResult::ok(AmpOp::Recall);
        out.hits = fused;
        Ok(out)
    }
}

/// Reciprocal Rank Fusion over several ranked hit lists.
///
/// Each hit contributes `1 / (k + rank)` (rank is 0-based within its
/// list) to its id's fused score; identical ids across lists sum. The
/// `k` damping is what makes RRF robust to a single adversarial top
/// rank: a rank-0 injection in one list adds only `1/(k+0)`, which a
/// genuinely-relevant item ranked highly across *multiple* lists still
/// beats. Returns hits sorted by fused score, highest first.
pub fn rrf_fuse(lists: &[Vec<AmpHit>], k: f32) -> Vec<AmpHit> {
    use std::collections::HashMap;
    let mut score: HashMap<String, f32> = HashMap::new();
    let mut repr: HashMap<String, AmpHit> = HashMap::new();
    for list in lists {
        for (rank, hit) in list.iter().enumerate() {
            *score.entry(hit.id.clone()).or_insert(0.0) += 1.0 / (k + rank as f32);
            repr.entry(hit.id.clone()).or_insert_with(|| hit.clone());
        }
    }
    sort_by_fused(score, repr)
}

/// Max-score fusion: an id's fused score is the single best score it
/// earned in any list. Simple, but a rank-0 adversarial injection with
/// an inflated score wins outright — the failure mode the conformance
/// test contrasts against RRF.
pub fn max_fuse(lists: &[Vec<AmpHit>]) -> Vec<AmpHit> {
    use std::collections::HashMap;
    let mut score: HashMap<String, f32> = HashMap::new();
    let mut repr: HashMap<String, AmpHit> = HashMap::new();
    for list in lists {
        for hit in list {
            let e = score.entry(hit.id.clone()).or_insert(f32::MIN);
            if hit.score > *e {
                *e = hit.score;
            }
            repr.entry(hit.id.clone()).or_insert_with(|| hit.clone());
        }
    }
    sort_by_fused(score, repr)
}

fn sort_by_fused(
    score: std::collections::HashMap<String, f32>,
    repr: std::collections::HashMap<String, AmpHit>,
) -> Vec<AmpHit> {
    let mut fused: Vec<AmpHit> = repr
        .into_iter()
        .map(|(id, mut hit)| {
            hit.score = score.get(&id).copied().unwrap_or(0.0);
            hit
        })
        .collect();
    fused.sort_by(|a, b| {
        // Deterministic: score desc, then id asc to break ties.
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    fused
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::AmpMemoryType;

    fn hit(id: &str, score: f32) -> AmpHit {
        AmpHit {
            id: id.to_string(),
            content: format!("content-{id}"),
            memory_type: AmpMemoryType::Semantic,
            score,
            tags: vec![],
        }
    }

    #[test]
    fn rrf_holds_under_rank0_injection_but_max_is_fooled() {
        // `TRUE` is the genuinely-relevant item: ranked high in BOTH
        // lists. `ADV` is an adversarial injection sitting at rank 0 of
        // list A only, with an inflated raw score.
        let list_a = vec![
            hit("ADV", 999.0), // rank 0, adversarial, huge score
            hit("TRUE", 0.9),  // rank 1
            hit("x1", 0.5),
        ];
        let list_b = vec![
            hit("TRUE", 0.95), // rank 0 in the honest list
            hit("y1", 0.6),
            hit("y2", 0.4),
        ];

        // RRF: TRUE scores 1/(60+1) + 1/(60+0); ADV scores 1/(60+0)
        // only. TRUE wins.
        let rrf = rrf_fuse(&[list_a.clone(), list_b.clone()], 60.0);
        assert_eq!(rrf[0].id, "TRUE", "RRF must rank the true item first");

        // Max-fusion: ADV's 999.0 is the single best score anywhere, so
        // the injection wins — the failure mode.
        let max = max_fuse(&[list_a, list_b]);
        assert_eq!(
            max[0].id, "ADV",
            "max-fusion is fooled by the rank-0 injection"
        );
    }

    #[test]
    fn rrf_is_deterministic() {
        let a = vec![hit("a", 0.9), hit("b", 0.8)];
        let b = vec![hit("b", 0.7), hit("a", 0.6)];
        let r1 = rrf_fuse(&[a.clone(), b.clone()], 60.0);
        let r2 = rrf_fuse(&[a, b], 60.0);
        assert_eq!(r1, r2);
    }
}
