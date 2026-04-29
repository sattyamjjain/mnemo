//! z-score + EWMA drift detector (v0.4.1 P0-3).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Notice,
    Warning,
    High,
    Critical,
}

impl Severity {
    pub fn from_z(z: f32) -> Self {
        let a = z.abs();
        match a {
            x if x >= 4.0 => Severity::Critical,
            x if x >= 3.0 => Severity::High,
            x if x >= 2.0 => Severity::Warning,
            x if x >= 1.0 => Severity::Notice,
            _ => Severity::Info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BaselineMetric {
    RecallRate,
    WriteRate,
    NamespaceFanout,
    ToolMix,
    HmacContinuity,
    ForgetRate,
}

impl BaselineMetric {
    pub fn as_str(&self) -> &'static str {
        match self {
            BaselineMetric::RecallRate => "recall_rate_per_min",
            BaselineMetric::WriteRate => "write_rate_per_min",
            BaselineMetric::NamespaceFanout => "namespace_fanout",
            BaselineMetric::ToolMix => "tool_mix_kl_divergence",
            BaselineMetric::HmacContinuity => "hmac_continuity",
            BaselineMetric::ForgetRate => "forget_rate_per_min",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineDelta {
    pub metric: BaselineMetric,
    pub z: f32,
    pub ewma_drift: f32,
    pub severity: Severity,
}

impl BaselineDelta {
    pub fn new(metric: BaselineMetric, z: f32, ewma_drift: f32) -> Self {
        Self {
            metric,
            z,
            ewma_drift,
            severity: Severity::from_z(z),
        }
    }
}

/// Compute the z-score of an observation against a rolling
/// (mean, stddev). Stddev floored at `1e-6` so a steady-state
/// metric (zero variance) doesn't divide by zero.
pub fn z_score(x: f32, mean: f32, stddev: f32) -> f32 {
    let s = stddev.max(1e-6);
    (x - mean) / s
}

/// Compute the EWMA drift between the live observation and the
/// historical mean, given alpha. Larger alpha = more weight on
/// recent obs.
pub fn ewma_drift(prev_ewma: f32, x: f32, alpha: f32) -> f32 {
    let a = alpha.clamp(0.0, 1.0);
    a * x + (1.0 - a) * prev_ewma
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_thresholds() {
        assert_eq!(Severity::from_z(0.5), Severity::Info);
        assert_eq!(Severity::from_z(1.5), Severity::Notice);
        assert_eq!(Severity::from_z(2.5), Severity::Warning);
        assert_eq!(Severity::from_z(3.5), Severity::High);
        assert_eq!(Severity::from_z(5.0), Severity::Critical);
    }

    #[test]
    fn z_score_handles_zero_variance() {
        // Steady-state metric shouldn't NaN.
        let z = z_score(2.0, 1.0, 0.0);
        assert!(z.is_finite());
    }

    #[test]
    fn ewma_clamps_alpha() {
        let d = ewma_drift(10.0, 100.0, 5.0);
        // Alpha clamped to 1.0 → result == observation.
        assert!((d - 100.0).abs() < 1e-3);
    }

    #[test]
    fn burst_flips_severity_to_high() {
        // 10x recall burst → e.g. observed 50/min, mean 5/min, stddev 2/min.
        let z = z_score(50.0, 5.0, 2.0);
        let sev = Severity::from_z(z);
        // 10x burst with 2.5σ stddev gives z~22 → Critical.
        assert!(sev == Severity::Critical || sev == Severity::High);
    }

    #[test]
    fn metric_strings_are_stable() {
        for m in [
            BaselineMetric::RecallRate,
            BaselineMetric::WriteRate,
            BaselineMetric::NamespaceFanout,
            BaselineMetric::ToolMix,
            BaselineMetric::HmacContinuity,
            BaselineMetric::ForgetRate,
        ] {
            assert!(!m.as_str().is_empty());
        }
    }
}
