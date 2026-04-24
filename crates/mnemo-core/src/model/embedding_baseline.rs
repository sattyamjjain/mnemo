use serde::{Deserialize, Serialize};

/// Per-agent embedding-space summary used by the z-score outlier detector.
///
/// `mu` is the mean vector; `cov_diag` is the per-dimension variance (we
/// store only the diagonal — a proxy for the full covariance matrix that
/// keeps storage and scoring O(d) rather than O(d^2)). `n` is the sample
/// count the baseline was trained on; scoring ignores a baseline with
/// fewer than `MIN_BASELINE_SAMPLES` samples because variance estimates
/// below that threshold are too noisy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingBaseline {
    pub agent_id: String,
    pub mu: Vec<f32>,
    pub cov_diag: Vec<f32>,
    pub n: u64,
    pub updated_at: String,
}

/// Samples below this count are considered too noisy to score against.
pub const MIN_BASELINE_SAMPLES: u64 = 30;
