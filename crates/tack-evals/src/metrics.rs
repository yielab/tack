//! The numbers a report is made of, as pure functions of counts and scores.
//!
//! The definitions are the ones the agent-eval literature settled on: the unbiased pass@k
//! estimator (Chen et al.), its strict counterpart pass^k, the Wilson score interval, and the
//! error bars of Miller's "Adding error bars to evals" (clustered standard error; paired
//! difference). Each is pinned by a hand-computed value in the tests.

/// The normal quantile of a 95 % two-sided interval.
pub const Z95: f64 = 1.96;

/// The chance that at least one of `k` attempts drawn from `n` succeeds, given `c` of the `n`
/// did. `None` when fewer than `k` attempts were made.
pub fn pass_at_k(n: u32, c: u32, k: u32) -> Option<f64> {
    if k == 0 || n < k || c > n {
        return None;
    }
    if n - c < k {
        return Some(1.0);
    }
    // 1 - C(n-c, k) / C(n, k), as a product so it does not overflow.
    let miss: f64 = (n - c + 1..=n)
        .map(|i| 1.0 - f64::from(k) / f64::from(i))
        .product();
    Some(1.0 - miss)
}

/// The chance that all `k` attempts drawn from `n` succeed, given `c` of the `n` did:
/// C(c, k) / C(n, k). `None` when fewer than `k` attempts were made.
pub fn pass_pow_k(n: u32, c: u32, k: u32) -> Option<f64> {
    if k == 0 || n < k || c > n {
        return None;
    }
    if c < k {
        return Some(0.0);
    }
    Some(
        (0..k)
            .map(|i| f64::from(c - i) / f64::from(n - i))
            .product(),
    )
}

/// The Wilson score interval for `successes` of `n` at 95 %. `None` for no trials.
pub fn wilson(successes: u32, n: u32) -> Option<(f64, f64)> {
    if n == 0 || successes > n {
        return None;
    }
    let n = f64::from(n);
    let p = f64::from(successes) / n;
    let z2 = Z95 * Z95;
    let denominator = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denominator;
    let half = Z95 * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denominator;
    Some(((center - half).max(0.0), (center + half).min(1.0)))
}

/// The standard error of the mean of `scores` when they come in clusters (the trials of one
/// task are not independent draws): the plain standard error plus the covariance of each
/// cluster's members. `None` below two scores.
pub fn clustered_standard_error(clusters: &[Vec<f64>]) -> Option<f64> {
    let total: usize = clusters.iter().map(Vec::len).sum();
    if total < 2 {
        return None;
    }
    let count = total as f64;
    let mean = clusters.iter().flatten().sum::<f64>() / count;
    let squares: f64 = clusters.iter().flatten().map(|s| (s - mean).powi(2)).sum();
    let plain = squares / (count - 1.0) / count;
    // For a cluster, the sum over i != j of (s_i - mean)(s_j - mean) is the square of the
    // sum of the deviations minus the sum of their squares.
    let covariance: f64 = clusters
        .iter()
        .map(|cluster| {
            let deviations: Vec<f64> = cluster.iter().map(|s| s - mean).collect();
            deviations.iter().sum::<f64>().powi(2) - deviations.iter().map(|d| d * d).sum::<f64>()
        })
        .sum::<f64>()
        / (count * count);
    Some((plain + covariance).max(0.0).sqrt())
}

/// A difference between two systems on the same tasks, with its 95 % interval.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PairedDifference {
    pub mean: f64,
    pub standard_error: f64,
    pub low: f64,
    pub high: f64,
}

impl PairedDifference {
    /// True when the interval holds zero: the data cannot tell the systems apart.
    pub fn crosses_zero(&self) -> bool {
        self.low <= 0.0 && self.high >= 0.0
    }
}

/// `head - base` over tasks, each task's score being the mean of its trials: the standard
/// error is that of the per-task differences, so the correlation between the two systems on
/// one task is used and not thrown away. `None` below two tasks.
pub fn paired_difference(base: &[f64], head: &[f64]) -> Option<PairedDifference> {
    let n = base.len();
    if n < 2 || head.len() != n {
        return None;
    }
    let count = n as f64;
    let differences: Vec<f64> = head.iter().zip(base).map(|(h, b)| h - b).collect();
    let mean = differences.iter().sum::<f64>() / count;
    let variance = differences.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (count - 1.0);
    let standard_error = (variance / count).sqrt();
    Some(PairedDifference {
        mean,
        standard_error,
        low: mean - Z95 * standard_error,
        high: mean + Z95 * standard_error,
    })
}

/// The `fraction` quantile of `values` by the nearest-rank rule (p50 of four values is the
/// second). `None` for no values.
pub fn percentile(values: &[u64], fraction: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (fraction * sorted.len() as f64).ceil().max(1.0) as usize;
    Some(sorted[rank.min(sorted.len()) - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-3, "{got} is not {want}");
    }

    #[test]
    fn pass_at_k_and_pass_pow_k_match_the_hand_values() {
        // 5 attempts, 3 good, 2 drawn: 1 - C(2,2)/C(5,2) = 0.9 and C(3,2)/C(5,2) = 0.3.
        close(pass_at_k(5, 3, 2).unwrap(), 0.9);
        close(pass_pow_k(5, 3, 2).unwrap(), 0.3);
        // One draw is the plain rate in both.
        close(pass_at_k(5, 3, 1).unwrap(), 0.6);
        close(pass_pow_k(5, 3, 1).unwrap(), 0.6);
        // Drawing as many as there are: any good one counts, all must be good.
        close(pass_at_k(3, 1, 3).unwrap(), 1.0);
        close(pass_pow_k(3, 2, 3).unwrap(), 0.0);
        close(pass_pow_k(3, 3, 3).unwrap(), 1.0);
        close(pass_at_k(3, 0, 2).unwrap(), 0.0);
    }

    #[test]
    fn pass_at_k_needs_k_attempts() {
        assert_eq!(pass_at_k(2, 1, 3), None);
        assert_eq!(pass_pow_k(2, 1, 3), None);
        assert_eq!(pass_at_k(2, 1, 0), None);
    }

    #[test]
    fn wilson_matches_a_table() {
        // 95 % Wilson intervals, as tabulated.
        for (successes, n, low, high) in [
            (8, 10, 0.4902, 0.9433),
            (0, 10, 0.0, 0.2775),
            (10, 10, 0.7225, 1.0),
            (1, 3, 0.0615, 0.7923),
            (50, 100, 0.4038, 0.5962),
        ] {
            let (got_low, got_high) = wilson(successes, n).unwrap();
            assert!(
                (got_low - low).abs() < 1e-3,
                "{successes}/{n} low {got_low}"
            );
            assert!(
                (got_high - high).abs() < 1e-3,
                "{successes}/{n} high {got_high}"
            );
        }
        assert_eq!(wilson(0, 0), None);
    }

    #[test]
    fn clustered_error_is_wider_than_the_plain_one_when_a_task_is_all_or_nothing() {
        // Two tasks, three trials each: one always passes, one never does. Treated as six
        // independent draws the mean would look well known; it rests on two tasks.
        let clusters = vec![vec![1.0, 1.0, 1.0], vec![0.0, 0.0, 0.0]];
        let clustered = clustered_standard_error(&clusters).unwrap();
        // Plain: variance 0.3 / 6 -> 0.2236. Covariance: each cluster's deviations sum to
        // +-1.5, so (2.25 - 0.75) twice over 36 -> 0.0833. Together sqrt(0.05 + 0.0833).
        close(clustered, (0.3_f64 / 6.0 + 2.0 * 1.5 / 36.0).sqrt());
        // Independent clusters of one are the plain standard error.
        let single = vec![vec![1.0], vec![0.0], vec![1.0], vec![1.0]];
        close(
            clustered_standard_error(&single).unwrap(),
            (0.25_f64 / 4.0).sqrt(),
        );
        assert_eq!(clustered_standard_error(&[vec![1.0]]), None);
    }

    #[test]
    fn the_paired_difference_matches_a_worked_example() {
        // Four tasks, each scored by the share of its trials that passed.
        let base = [1.0, 0.0, 0.0, 0.0];
        let head = [1.0, 1.0, 0.0, 1.0];
        // Differences 0, 1, 0, 1: mean 0.5, sample variance 1/3, standard error
        // sqrt(1/3 / 4) = 0.2887, interval 0.5 +- 0.5658.
        let got = paired_difference(&base, &head).unwrap();
        close(got.mean, 0.5);
        close(got.standard_error, 0.2887);
        close(got.low, -0.0658);
        close(got.high, 1.0658);
        assert!(got.crosses_zero());
    }

    #[test]
    fn a_consistent_gap_does_not_cross_zero() {
        let base = [0.0, 0.2, 0.1, 0.0, 0.3, 0.1];
        let head: Vec<f64> = base.iter().map(|b| b + 0.3).collect();
        let got = paired_difference(&base, &head).unwrap();
        close(got.mean, 0.3);
        assert!(!got.crosses_zero());
        assert_eq!(paired_difference(&[1.0], &[0.0]), None);
    }

    #[test]
    fn percentiles_use_the_nearest_rank() {
        let values = [40, 10, 30, 20];
        assert_eq!(percentile(&values, 0.5), Some(20));
        assert_eq!(percentile(&values, 0.9), Some(40));
        assert_eq!(percentile(&[7], 0.5), Some(7));
        assert_eq!(percentile(&[], 0.5), None);
    }
}
