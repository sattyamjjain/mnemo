use serde::{Deserialize, Serialize};

use crate::model::embedding_baseline::{EmbeddingBaseline, MIN_BASELINE_SAMPLES};
use crate::model::memory::MemoryRecord;

/// Floor added to every per-dimension variance to stop the z-score from
/// exploding on degenerate dimensions (constant values produce variance
/// 0 which would otherwise divide by zero). Chosen an order of magnitude
/// below the smallest variance observed across OpenAI + ONNX + MiniLM
/// embeddings in the mnemo test corpus.
const VARIANCE_FLOOR: f32 = 1e-6;

/// Result of scoring one record against a baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlierScore {
    /// Normalised Mahalanobis-proxy: `sqrt(mean over dims of ((x - mu)^2 / var))`.
    /// Normalisation by dimension count keeps the score comparable across
    /// embedding sizes.
    pub z_score: f32,
    /// Threshold the caller supplied; `is_outlier = z_score >= threshold`.
    pub threshold: f32,
    /// Convenience flag.
    pub is_outlier: bool,
    /// Count of dimensions whose individual squared z-score exceeded 9.0
    /// (i.e. >=3 standard deviations). A high count on long vectors is a
    /// useful tie-breaker: a single rogue dimension can lift the mean
    /// Mahalanobis without representing a real distribution shift.
    pub dims_flagged: u32,
    /// Baseline sample count — surfaced so callers can reason about
    /// reliability (below `MIN_BASELINE_SAMPLES` the score is still
    /// computed but `is_outlier` is pinned to `false`).
    pub baseline_n: u64,
}

impl OutlierScore {
    pub fn no_baseline(threshold: f32) -> Self {
        Self {
            z_score: 0.0,
            threshold,
            is_outlier: false,
            dims_flagged: 0,
            baseline_n: 0,
        }
    }
}

/// Score a single record's embedding against a trained baseline.
///
/// Returns a no-op score when:
/// * the record has no embedding,
/// * the baseline's dimensionality disagrees with the record's, or
/// * the baseline holds fewer than [`MIN_BASELINE_SAMPLES`] samples.
///
/// Otherwise computes the mean per-dimension squared z-score and returns
/// its square root, which is the standard normalised Mahalanobis proxy
/// used in outlier-detection literature when only the diagonal is
/// available.
pub fn score_embedding_outlier(
    record: &MemoryRecord,
    baseline: &EmbeddingBaseline,
    threshold: f32,
) -> OutlierScore {
    let Some(embedding) = record.embedding.as_ref() else {
        return OutlierScore::no_baseline(threshold);
    };
    if embedding.len() != baseline.mu.len() || embedding.len() != baseline.cov_diag.len() {
        return OutlierScore::no_baseline(threshold);
    }
    if baseline.n < MIN_BASELINE_SAMPLES {
        return OutlierScore {
            z_score: 0.0,
            threshold,
            is_outlier: false,
            dims_flagged: 0,
            baseline_n: baseline.n,
        };
    }

    let d = embedding.len() as f32;
    let mut sum_sq = 0.0f32;
    let mut dims_flagged: u32 = 0;
    for i in 0..embedding.len() {
        let diff = embedding[i] - baseline.mu[i];
        let var = baseline.cov_diag[i].max(VARIANCE_FLOOR);
        let sq_z = (diff * diff) / var;
        if sq_z >= 9.0 {
            dims_flagged += 1;
        }
        sum_sq += sq_z;
    }
    let z_score = (sum_sq / d).sqrt();
    OutlierScore {
        z_score,
        threshold,
        is_outlier: z_score >= threshold,
        dims_flagged,
        baseline_n: baseline.n,
    }
}

/// Compute a fresh baseline from a slice of records. Records without
/// embeddings are skipped; if fewer than 2 survive the function returns
/// `None` — a baseline of 1 sample has zero variance everywhere and
/// would pin `is_outlier` to `false` on every subsequent record anyway.
///
/// Variance is computed with Welford's online algorithm in one pass;
/// although we don't need the online property here it's numerically
/// stabler than the naive two-pass form on large batches.
pub fn train_baseline(agent_id: &str, records: &[MemoryRecord]) -> Option<EmbeddingBaseline> {
    let mut records_with_emb = records
        .iter()
        .filter_map(|r| r.embedding.as_ref().map(|e| (r, e)));

    let (_first_record, first_emb) = records_with_emb.next()?;
    let d = first_emb.len();
    if d == 0 {
        return None;
    }
    let mut count: u64 = 1;
    let mut mean: Vec<f32> = first_emb.clone();
    let mut m2: Vec<f32> = vec![0.0; d];

    for (_r, emb) in records_with_emb {
        if emb.len() != d {
            continue; // skip dim-mismatched records silently
        }
        count += 1;
        let n = count as f32;
        for i in 0..d {
            let x = emb[i];
            let delta = x - mean[i];
            mean[i] += delta / n;
            let delta2 = x - mean[i];
            m2[i] += delta * delta2;
        }
    }

    if count < 2 {
        return None;
    }

    let divisor = (count - 1) as f32;
    let cov_diag: Vec<f32> = m2.iter().map(|v| v / divisor).collect();

    Some(EmbeddingBaseline {
        agent_id: agent_id.to_string(),
        mu: mean,
        cov_diag,
        n: count,
        updated_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::memory::MemoryRecord;

    fn record_with_embedding(embedding: Vec<f32>) -> MemoryRecord {
        let mut r = MemoryRecord::new("test-agent".to_string(), "x".to_string());
        r.embedding = Some(embedding);
        r
    }

    fn make_records(mean: f32, stddev: f32, n: usize, d: usize) -> Vec<MemoryRecord> {
        // Simple deterministic pseudo-normal draw using alternating offsets
        // — avoids pulling a full PRNG dep for a unit test.
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            let magnitude = stddev * ((i as f32 / n as f32).sin().abs() + 0.5);
            let emb: Vec<f32> = (0..d)
                .map(|k| mean + sign * magnitude + k as f32 * 0.001)
                .collect();
            out.push(record_with_embedding(emb));
        }
        out
    }

    #[test]
    fn trains_baseline_from_records() {
        let records = make_records(0.1, 0.05, 40, 8);
        let baseline = train_baseline("test-agent", &records).expect("baseline");
        assert_eq!(baseline.mu.len(), 8);
        assert_eq!(baseline.cov_diag.len(), 8);
        assert_eq!(baseline.n, 40);
        assert_eq!(baseline.agent_id, "test-agent");
    }

    #[test]
    fn returns_none_on_no_embeddings() {
        let mut record = record_with_embedding(vec![0.1; 4]);
        record.embedding = None;
        assert!(train_baseline("a", &[record]).is_none());
    }

    #[test]
    fn in_distribution_not_flagged() {
        let records = make_records(0.1, 0.05, 60, 16);
        let baseline = train_baseline("a", &records).unwrap();
        // Score one of the training records — must not be flagged.
        let score = score_embedding_outlier(&records[5], &baseline, 3.0);
        assert!(
            !score.is_outlier,
            "in-distribution record flagged: z={} dims_flagged={}",
            score.z_score, score.dims_flagged
        );
    }

    #[test]
    fn far_out_of_distribution_flagged() {
        let records = make_records(0.1, 0.05, 60, 16);
        let baseline = train_baseline("a", &records).unwrap();
        // Construct a record 50 stddevs away in every dimension.
        let mut attacker = records[0].clone();
        let mu0 = baseline.mu[0];
        let stddev0 = baseline.cov_diag[0].sqrt();
        let push = mu0 + 50.0 * stddev0.max(0.01);
        attacker.embedding = Some(vec![push; 16]);
        let score = score_embedding_outlier(&attacker, &baseline, 3.0);
        assert!(
            score.is_outlier,
            "far-OOD record not flagged: z={} threshold={}",
            score.z_score, score.threshold
        );
    }

    #[test]
    fn noisy_baseline_pins_is_outlier_false() {
        let records = make_records(0.1, 0.05, 5, 8);
        // Train with too few samples.
        let baseline = train_baseline("a", &records).unwrap();
        let score = score_embedding_outlier(&records[0], &baseline, 3.0);
        assert!(
            !score.is_outlier,
            "noisy baseline should pin is_outlier=false"
        );
        assert!(score.baseline_n < MIN_BASELINE_SAMPLES);
    }

    #[test]
    fn dim_mismatch_returns_no_op() {
        let records = make_records(0.1, 0.05, 40, 8);
        let baseline = train_baseline("a", &records).unwrap();
        let mut mismatched = records[0].clone();
        mismatched.embedding = Some(vec![0.1; 16]);
        let score = score_embedding_outlier(&mismatched, &baseline, 3.0);
        assert_eq!(score.z_score, 0.0);
        assert!(!score.is_outlier);
    }
}
