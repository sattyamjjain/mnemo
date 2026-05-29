//! Feedback-driven consolidation trigger metric.
//!
//! Opt-in scalar "maturity / generalizability" score per memory cluster
//! that gates [`crate::query::lifecycle::run_consolidation`] when the
//! engine is configured with
//! [`ConsolidationPolicy::MaturityDriven`]. The fixed-schedule /
//! min-cluster-size behaviour remains the default — this module is
//! purely additive.
//!
//! # Score
//!
//! Four components, each normalised to `[0, 1]` and combined as a
//! weight-normalised sum so a zeroed weight contributes nothing rather
//! than re-weighting the rest:
//!
//! - `recency` — mean access-recency across the cluster, decayed by
//!   `recency_half_life_hours`. A cluster whose members were touched
//!   recently scores higher.
//! - `hit_success` — mean `access_count`, log-scaled to `[0, 1]` via
//!   `ln(1 + n) / ln(1 + hit_saturation)`.
//! - `edge_degree` — mean (incoming + outgoing) graph-relation count
//!   per cluster member, normalised by `degree_saturation`.
//! - `redundancy` — mean pairwise cosine similarity of cluster
//!   embeddings (skipped when fewer than two members have embeddings,
//!   in which case the component is `0.5` — neutral, neither helpful
//!   nor harmful).
//!
//! Consolidation fires when `combined >= threshold` AND the cluster has
//! at least `min_cluster_size_floor` members.
//!
//! # Prior art
//!
//! Internal note (not user-facing): FluxMem (arXiv:2605.28773) frames
//! consolidation as a feedback-driven control loop with a scalar
//! maturity score gating the trigger. mnemo's policy is a structural
//! cousin — same four-axis intuition (recency, hit-feedback, graph
//! degree, neighbour redundancy) wired into the existing tag-overlap
//! clusterer rather than a learned cluster-er. Cited as prior art only.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::memory::MemoryRecord;
use crate::query::MnemoEngine;

/// Per-component weights for the maturity score.
///
/// All weights are clamped to `[0, 1]`. The combined score normalises
/// by the sum of weights, so setting a weight to `0` disables that
/// component without inflating the others.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MaturityWeights {
    pub recency: f32,
    pub hit_success: f32,
    pub edge_degree: f32,
    pub redundancy: f32,
}

impl MaturityWeights {
    /// Balanced default: hit-success leads, recency + redundancy
    /// roughly equal, edge-degree lower because it depends on relation
    /// curation which is operator-driven.
    pub const fn balanced() -> Self {
        Self {
            recency: 0.25,
            hit_success: 0.30,
            edge_degree: 0.20,
            redundancy: 0.25,
        }
    }

    fn clamped(self) -> Self {
        Self {
            recency: self.recency.clamp(0.0, 1.0),
            hit_success: self.hit_success.clamp(0.0, 1.0),
            edge_degree: self.edge_degree.clamp(0.0, 1.0),
            redundancy: self.redundancy.clamp(0.0, 1.0),
        }
    }

    fn sum(self) -> f32 {
        self.recency + self.hit_success + self.edge_degree + self.redundancy
    }
}

impl Default for MaturityWeights {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Per-component maturity score breakdown, returned alongside the
/// combined score for observability.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MaturityBreakdown {
    pub recency: f32,
    pub hit_success: f32,
    pub edge_degree: f32,
    pub redundancy: f32,
    pub combined: f32,
}

/// Tunable saturation knobs for the four normalised components. Defaults
/// are chosen so a "typical" working set lands near `0.5` on each axis.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MaturitySaturation {
    /// Hours of half-life used by the recency exponential. A mean
    /// access-age of `recency_half_life_hours` maps to `0.5`.
    pub recency_half_life_hours: f32,
    /// Log saturation point for `hit_success`. A mean `access_count` of
    /// `hit_saturation` maps to `1.0`.
    pub hit_saturation: f32,
    /// Mean (in + out) edges per record that map to `1.0`.
    pub degree_saturation: f32,
}

impl Default for MaturitySaturation {
    fn default() -> Self {
        Self {
            recency_half_life_hours: 72.0,
            hit_saturation: 8.0,
            degree_saturation: 6.0,
        }
    }
}

/// Feedback-driven consolidation policy. The cluster is consolidated
/// iff `combined >= threshold` and it has at least
/// `min_cluster_size_floor` members.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaturityPolicy {
    pub weights: MaturityWeights,
    pub saturation: MaturitySaturation,
    /// Combined-score gate, in `[0, 1]`.
    pub threshold: f32,
    /// Minimum cluster size still enforced even when the metric clears
    /// the threshold. Acts as a hard floor against pathological 1-record
    /// "clusters".
    pub min_cluster_size_floor: usize,
    /// Run consolidation opportunistically after every successful
    /// `forget::execute`. Best-effort; failures are logged, never
    /// propagated to the forget caller.
    pub trigger_on_forget: bool,
    /// Run consolidation opportunistically after every successful
    /// `checkpoint::execute`. Best-effort; failures are logged.
    pub trigger_on_checkpoint: bool,
}

impl MaturityPolicy {
    /// Conservative starter policy: balanced weights, default
    /// saturations, threshold = 0.55, floor = 2, hook only on forget.
    pub fn balanced() -> Self {
        Self {
            weights: MaturityWeights::balanced(),
            saturation: MaturitySaturation::default(),
            threshold: 0.55,
            min_cluster_size_floor: 2,
            trigger_on_forget: true,
            trigger_on_checkpoint: false,
        }
    }
}

impl Default for MaturityPolicy {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Policy for [`crate::query::lifecycle::run_consolidation`].
///
/// - `FixedSize` (default): preserves the v0.4.x behaviour — every
///   tag-overlap cluster with at least `min_cluster_size` members is
///   consolidated unconditionally.
/// - `MaturityDriven`: the additional feedback-driven trigger metric
///   from this module gates each cluster.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConsolidationPolicy {
    #[default]
    FixedSize,
    MaturityDriven(MaturityPolicy),
}

/// Compute the per-cluster maturity breakdown.
///
/// Returns `None` for empty clusters (caller should never invoke on
/// one, but we degrade gracefully).
pub async fn compute_cluster_maturity(
    engine: &MnemoEngine,
    cluster: &[&MemoryRecord],
    weights: MaturityWeights,
    saturation: MaturitySaturation,
) -> Result<Option<MaturityBreakdown>> {
    if cluster.is_empty() {
        return Ok(None);
    }
    let weights = weights.clamped();
    let recency = recency_component(cluster, saturation.recency_half_life_hours);
    let hit_success = hit_success_component(cluster, saturation.hit_saturation);
    let edge_degree = edge_degree_component(engine, cluster, saturation.degree_saturation).await?;
    let redundancy = redundancy_component(cluster);

    let combined = combined_score(
        weights,
        MaturityBreakdown {
            recency,
            hit_success,
            edge_degree,
            redundancy,
            combined: 0.0,
        },
    );

    Ok(Some(MaturityBreakdown {
        recency,
        hit_success,
        edge_degree,
        redundancy,
        combined,
    }))
}

fn combined_score(weights: MaturityWeights, b: MaturityBreakdown) -> f32 {
    let total = weights.sum();
    if total <= f32::EPSILON {
        return 0.0;
    }
    let mixed = weights.recency * b.recency
        + weights.hit_success * b.hit_success
        + weights.edge_degree * b.edge_degree
        + weights.redundancy * b.redundancy;
    (mixed / total).clamp(0.0, 1.0)
}

fn recency_component(cluster: &[&MemoryRecord], half_life_hours: f32) -> f32 {
    if cluster.is_empty() {
        return 0.0;
    }
    let half_life = half_life_hours.max(f32::EPSILON);
    // Decay constant chosen so age == half_life maps to 0.5.
    let lambda = std::f32::consts::LN_2 / half_life;
    let now = chrono::Utc::now();
    let mut sum = 0.0_f32;
    let mut n = 0_u32;
    for r in cluster {
        let last = r.last_accessed_at.as_deref().unwrap_or(&r.created_at);
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(last) {
            let hours = (now - dt.with_timezone(&chrono::Utc)).num_seconds().max(0) as f32 / 3600.0;
            sum += (-lambda * hours).exp();
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { sum / n as f32 }
}

fn hit_success_component(cluster: &[&MemoryRecord], hit_saturation: f32) -> f32 {
    if cluster.is_empty() {
        return 0.0;
    }
    let sat = hit_saturation.max(1.0);
    let denom = (1.0 + sat).ln().max(f32::EPSILON);
    let mut sum = 0.0_f32;
    for r in cluster {
        sum += (1.0 + r.access_count as f32).ln() / denom;
    }
    (sum / cluster.len() as f32).clamp(0.0, 1.0)
}

async fn edge_degree_component(
    engine: &MnemoEngine,
    cluster: &[&MemoryRecord],
    degree_saturation: f32,
) -> Result<f32> {
    if cluster.is_empty() {
        return Ok(0.0);
    }
    let sat = degree_saturation.max(1.0);
    let mut total_degree = 0_usize;
    for r in cluster {
        let outgoing = engine.storage.get_relations_from(r.id).await?.len();
        let incoming = engine.storage.get_relations_to(r.id).await?.len();
        total_degree += outgoing + incoming;
    }
    let mean = total_degree as f32 / cluster.len() as f32;
    Ok((mean / sat).clamp(0.0, 1.0))
}

fn redundancy_component(cluster: &[&MemoryRecord]) -> f32 {
    let with_emb: Vec<&Vec<f32>> = cluster
        .iter()
        .filter_map(|r| r.embedding.as_ref())
        .collect();
    if with_emb.len() < 2 {
        // Neutral when we cannot measure: do not punish or reward.
        return 0.5;
    }
    let mut sum = 0.0_f32;
    let mut pairs = 0_u32;
    for i in 0..with_emb.len() {
        for j in (i + 1)..with_emb.len() {
            if with_emb[i].len() != with_emb[j].len() || with_emb[i].is_empty() {
                continue;
            }
            sum += cosine_similarity(with_emb[i], with_emb[j]);
            pairs += 1;
        }
    }
    if pairs == 0 {
        0.5
    } else {
        // cosine ∈ [-1, 1]; clamp to [0, 1] so anti-correlated pairs
        // count as zero redundancy rather than negative.
        (sum / pairs as f32).clamp(0.0, 1.0)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
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
    use crate::embedding::NoopEmbedding;
    use crate::index::usearch::UsearchIndex;
    use crate::model::memory::{ConsolidationState, MemoryType, Scope, SourceType};
    use crate::storage::duckdb::DuckDbStorage;
    use std::sync::Arc;
    use uuid::Uuid;

    fn record(now: chrono::DateTime<chrono::Utc>, hours_ago: i64, access: u64) -> MemoryRecord {
        let created = (now - chrono::Duration::hours(hours_ago)).to_rfc3339();
        MemoryRecord {
            id: Uuid::now_v7(),
            agent_id: "a".to_string(),
            content: "c".to_string(),
            memory_type: MemoryType::Episodic,
            scope: Scope::Private,
            importance: 0.5,
            tags: vec![],
            metadata: serde_json::json!({}),
            embedding: None,
            content_hash: vec![],
            prev_hash: None,
            source_type: SourceType::Agent,
            source_id: None,
            consolidation_state: ConsolidationState::Raw,
            access_count: access,
            org_id: None,
            thread_id: None,
            created_at: created.clone(),
            updated_at: created,
            last_accessed_at: None,
            expires_at: None,
            deleted_at: None,
            decay_rate: None,
            created_by: None,
            version: 1,
            prev_version_id: None,
            quarantined: false,
            quarantine_reason: None,
            decay_function: None,
        }
    }

    #[test]
    fn recency_decays_with_age() {
        let now = chrono::Utc::now();
        let fresh = record(now, 0, 0);
        let stale = record(now, 200, 0);
        let fresh_score = recency_component(&[&fresh], 72.0);
        let stale_score = recency_component(&[&stale], 72.0);
        assert!(
            fresh_score > stale_score,
            "fresh {fresh_score} > stale {stale_score}"
        );
        // At t == half_life the score should be ~0.5.
        let half = record(now, 72, 0);
        let half_score = recency_component(&[&half], 72.0);
        assert!(
            (half_score - 0.5).abs() < 0.05,
            "half-life mapping: {half_score} ≈ 0.5"
        );
    }

    #[test]
    fn hit_success_saturates() {
        let now = chrono::Utc::now();
        let none = record(now, 0, 0);
        let some = record(now, 0, 4);
        let many = record(now, 0, 64);
        let s0 = hit_success_component(&[&none], 8.0);
        let s1 = hit_success_component(&[&some], 8.0);
        let s2 = hit_success_component(&[&many], 8.0);
        assert!(s0 < s1 && s1 <= s2);
        assert!(s2 <= 1.0);
        assert_eq!(s0, 0.0);
    }

    #[test]
    fn redundancy_handles_short_input() {
        let now = chrono::Utc::now();
        let r = record(now, 0, 0);
        // Single record → neutral 0.5.
        let s = redundancy_component(&[&r]);
        assert_eq!(s, 0.5);
    }

    #[test]
    fn redundancy_detects_identical_embeddings() {
        let now = chrono::Utc::now();
        let mut a = record(now, 0, 0);
        let mut b = record(now, 0, 0);
        a.embedding = Some(vec![1.0, 0.0, 0.0]);
        b.embedding = Some(vec![1.0, 0.0, 0.0]);
        let s = redundancy_component(&[&a, &b]);
        assert!((s - 1.0).abs() < 1e-5, "identical → 1.0, got {s}");
    }

    #[test]
    fn redundancy_orthogonal_is_zero() {
        let now = chrono::Utc::now();
        let mut a = record(now, 0, 0);
        let mut b = record(now, 0, 0);
        a.embedding = Some(vec![1.0, 0.0]);
        b.embedding = Some(vec![0.0, 1.0]);
        let s = redundancy_component(&[&a, &b]);
        assert!(s.abs() < 1e-5, "orthogonal → 0, got {s}");
    }

    #[test]
    fn combined_normalises_by_weight_sum() {
        // Only hit_success is weighted; other components must not pull
        // the combined score down.
        let weights = MaturityWeights {
            recency: 0.0,
            hit_success: 1.0,
            edge_degree: 0.0,
            redundancy: 0.0,
        };
        let b = MaturityBreakdown {
            recency: 0.0,
            hit_success: 0.9,
            edge_degree: 0.0,
            redundancy: 0.0,
            combined: 0.0,
        };
        let c = combined_score(weights, b);
        assert!((c - 0.9).abs() < 1e-5, "expected 0.9, got {c}");
    }

    #[test]
    fn combined_all_zero_weights_is_zero() {
        let weights = MaturityWeights {
            recency: 0.0,
            hit_success: 0.0,
            edge_degree: 0.0,
            redundancy: 0.0,
        };
        let b = MaturityBreakdown {
            recency: 1.0,
            hit_success: 1.0,
            edge_degree: 1.0,
            redundancy: 1.0,
            combined: 0.0,
        };
        assert_eq!(combined_score(weights, b), 0.0);
    }

    #[tokio::test]
    async fn edge_degree_component_zero_when_no_relations() {
        let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
        let index = Arc::new(UsearchIndex::new(3).unwrap());
        let embedding = Arc::new(NoopEmbedding::new(3));
        let engine = MnemoEngine::new(storage, index, embedding, "a".to_string(), None);
        let r = record(chrono::Utc::now(), 0, 0);
        engine.storage.insert_memory(&r).await.unwrap();
        let score = edge_degree_component(&engine, &[&r], 6.0).await.unwrap();
        assert_eq!(score, 0.0);
    }
}
