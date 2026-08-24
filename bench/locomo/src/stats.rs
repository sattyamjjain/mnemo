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

/// Two-sided exact McNemar p-value for a paired binary comparison.
///
/// `b` = cases the first system got right and the second got wrong, `c` = the
/// reverse. Concordant pairs carry no information about which system is better
/// and are deliberately not arguments. Under H0 each discordant pair is a fair
/// coin, so this is the two-sided binomial tail on `min(b, c)` out of `b + c`.
///
/// Exact rather than the chi-square approximation because the discordant count
/// here is small (tens), which is precisely where the approximation drifts.
pub fn mcnemar_exact_p(b: usize, c: usize) -> f64 {
    let m = b + c;
    if m == 0 {
        // No discordant pairs: the two systems made identical decisions
        // everywhere, so there is no evidence of a difference.
        return 1.0;
    }
    // Sum the lower tail with exact integer binomial coefficients, then double.
    // C(m, k) for m <= ~1000 stays well inside f64's exact-integer range at the
    // sizes this bench produces; the loop form avoids overflowing a u64.
    let k = b.min(c);
    let mut coeff = 1.0f64; // C(m, 0)
    let mut tail = 0.0f64;
    for i in 0..=k {
        if i > 0 {
            coeff = coeff * ((m - i + 1) as f64) / (i as f64);
        }
        tail += coeff;
    }
    (2.0 * tail * 0.5f64.powi(m as i32)).min(1.0)
}

/// Deterministic SplitMix64. Present so the bootstrap below is reproducible
/// from the seed recorded in the result file rather than from whatever the OS
/// RNG happened to hand out — a bench number nobody can re-derive is a claim,
/// not a measurement.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Percentile bootstrap 95% CI for the mean of `values`, using `resamples`
/// draws from the fixed `seed`.
///
/// Used for the *paired* per-query difference, where each element is one
/// query's (system A − system B) score. Resampling queries preserves the
/// pairing, which is the whole point: two independent marginal intervals
/// cannot answer whether A beats B, because they discard which query is which.
///
/// Percentile bootstrap rather than a t-interval because the per-query
/// difference is bounded in [-1, 1] and lumpy at small n, so normality is the
/// assumption most likely to be wrong here.
pub fn bootstrap_mean_ci95(values: &[f64], resamples: usize, seed: u64) -> (f64, f64, f64) {
    let n = values.len();
    if n == 0 {
        return (0.0, 0.0, 0.0);
    }
    let mean = |xs: &[f64]| xs.iter().sum::<f64>() / xs.len() as f64;
    let point = mean(values);
    if resamples == 0 {
        return (point, point, point);
    }
    let mut state = seed;
    let mut means = Vec::with_capacity(resamples);
    for _ in 0..resamples {
        let mut acc = 0.0;
        for _ in 0..n {
            let idx = (splitmix64(&mut state) % n as u64) as usize;
            acc += values[idx];
        }
        means.push(acc / n as f64);
    }
    means.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| {
        let i = ((resamples as f64 - 1.0) * q).round() as usize;
        means[i.min(resamples - 1)]
    };
    (point, at(0.025), at(0.975))
}

#[cfg(test)]
mod tests {
    use super::{bootstrap_mean_ci95, mcnemar_exact_p, wilson_95};

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

    #[test]
    fn mcnemar_no_discordant_pairs_is_no_evidence() {
        assert_eq!(mcnemar_exact_p(0, 0), 1.0);
    }

    #[test]
    fn mcnemar_matches_hand_computed_binomial() {
        // All 12 discordant pairs favour one side: 2 * 0.5^12.
        let p = mcnemar_exact_p(12, 0);
        assert!(
            (p - 2.0 * 0.5f64.powi(12)).abs() < 1e-12,
            "expected 2*0.5^12, got {p}"
        );
        // Symmetric in its arguments: which system is "first" cannot change the
        // two-sided p-value.
        assert!((mcnemar_exact_p(12, 0) - mcnemar_exact_p(0, 12)).abs() < 1e-12);
        // An even split is the least possible evidence.
        assert!((mcnemar_exact_p(7, 7) - 1.0).abs() < 1e-12);
    }

    /// The reason this module grew a paired test at all.
    ///
    /// The published marginals (31/45 vs 19/45) fix `b - c = 12` but say
    /// nothing about `c`. Both endpoints of the feasible range are consistent
    /// with those same two numbers, and they land on opposite sides of 0.05 —
    /// so the marginals alone cannot decide it, and a guard that only checked
    /// marginals would be checking nothing.
    #[test]
    fn same_marginals_can_be_significant_or_not() {
        let best = mcnemar_exact_p(12, 0);
        let worst = mcnemar_exact_p(28, 16);
        assert!(best < 0.001, "c=0 endpoint should be decisive: {best}");
        assert!(worst > 0.05, "c=16 endpoint should not separate: {worst}");
    }

    #[test]
    fn bootstrap_is_deterministic_and_brackets_the_mean() {
        let d: Vec<f64> = (0..45).map(|i| if i < 30 { 1.0 } else { -1.0 }).collect();
        let (point, lo, hi) = bootstrap_mean_ci95(&d, 2000, 42);
        let (point2, lo2, hi2) = bootstrap_mean_ci95(&d, 2000, 42);
        assert_eq!((point, lo, hi), (point2, lo2, hi2), "same seed, same CI");
        assert!(lo < point && point < hi, "CI must bracket: [{lo},{hi}]");
        assert!(
            (point - (30.0 - 15.0) / 45.0).abs() < 1e-12,
            "point estimate is the plain mean"
        );
    }

    #[test]
    fn bootstrap_on_all_zero_differences_does_not_separate() {
        // Two systems that agree on every query must produce an interval that
        // contains zero, or the method is manufacturing a difference.
        let (point, lo, hi) = bootstrap_mean_ci95(&[0.0; 45], 2000, 7);
        assert_eq!(point, 0.0);
        assert!(lo <= 0.0 && hi >= 0.0, "must contain 0: [{lo},{hi}]");
    }
}
