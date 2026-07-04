//! Shared statistics helpers for the bench bins.
//!
//! Centralised so every bench reports intervals the same way instead of each
//! bin re-deriving the formula.

/// Wilson 95% score interval for `successes`/`n` (z = 1.96). Returns
/// `(low, high)` clamped to `[0, 1]`.
///
/// Preferred over the normal approximation for proportions near 0 or 1 and for
/// small `n` — which is exactly the regime the bench accuracy numbers live in.
pub fn wilson_95(successes: usize, n: usize) -> (f64, f64) {
    if n == 0 {
        return (0.0, 0.0);
    }
    let z = 1.959_963_984_540_054_f64;
    let n = n as f64;
    let p = successes as f64 / n;
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = (z / denom) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    ((center - margin).max(0.0), (center + margin).min(1.0))
}

#[cfg(test)]
mod tests {
    use super::wilson_95;

    #[test]
    fn zero_n_is_degenerate() {
        assert_eq!(wilson_95(0, 0), (0.0, 0.0));
    }

    #[test]
    fn interval_brackets_point_estimate() {
        let (lo, hi) = wilson_95(80, 100);
        assert!(
            lo < 0.80 && 0.80 < hi,
            "80/100 CI must bracket 0.8: [{lo}, {hi}]"
        );
        assert!(lo >= 0.0 && hi <= 1.0);
    }

    #[test]
    fn perfect_score_upper_is_one_lower_below_one() {
        let (lo, hi) = wilson_95(50, 50);
        assert!((hi - 1.0).abs() < 1e-9);
        assert!(lo < 1.0, "a finite sample can't prove 100%: lo={lo}");
    }
}
