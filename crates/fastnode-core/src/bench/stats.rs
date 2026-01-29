//! Statistics computation for benchmarks.

use super::BenchStats;

/// Compute statistics from a collection of duration samples (in nanoseconds).
///
/// The samples are sorted internally. Returns min, median (p50), p95, and max.
///
/// # Panics
/// Panics if `samples` is empty.
#[must_use]
pub fn compute_stats(samples: &[u64]) -> BenchStats {
    assert!(!samples.is_empty(), "samples must not be empty");

    let mut sorted: Vec<u64> = samples.to_vec();
    sorted.sort_unstable();

    let len = sorted.len();
    let min_ns = sorted[0];
    let max_ns = sorted[len - 1];

    // Median (50th percentile)
    let median_ns = percentile(&sorted, 50);

    // 95th percentile
    let p95_ns = percentile(&sorted, 95);

    BenchStats {
        min_ns,
        median_ns,
        p95_ns,
        max_ns,
    }
}

/// Compute the nth percentile from a sorted slice.
///
/// Uses the "nearest rank" method.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]
fn percentile(sorted: &[u64], p: u32) -> u64 {
    assert!(!sorted.is_empty());
    assert!(p <= 100);

    if p == 0 {
        return sorted[0];
    }
    if p == 100 {
        return sorted[sorted.len() - 1];
    }

    // Nearest rank method: ceil((p/100) * n)
    // Casts are safe: p <= 100, n is array length (reasonable size)
    let n = sorted.len();
    let rank = ((f64::from(p) / 100.0) * n as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(n - 1);

    sorted[index]
}

/// Compute the median of a slice of `u64` samples.
///
/// Separate from `compute_stats` to avoid semantic confusion with `*_ns` field names,
/// since this is used for CPU time in microseconds and RSS in bytes.
///
/// # Panics
/// Panics if `samples` is empty.
#[must_use]
pub fn compute_median(samples: &[u64]) -> u64 {
    assert!(!samples.is_empty(), "samples must not be empty");

    let mut sorted: Vec<u64> = samples.to_vec();
    sorted.sort_unstable();

    let len = sorted.len();
    if len % 2 == 1 {
        sorted[len / 2]
    } else {
        // Average of the two middle values
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_stats_single_sample() {
        let samples = vec![1000];
        let stats = compute_stats(&samples);

        assert_eq!(stats.min_ns, 1000);
        assert_eq!(stats.median_ns, 1000);
        assert_eq!(stats.p95_ns, 1000);
        assert_eq!(stats.max_ns, 1000);
    }

    #[test]
    fn test_compute_stats_two_samples() {
        let samples = vec![100, 200];
        let stats = compute_stats(&samples);

        assert_eq!(stats.min_ns, 100);
        assert_eq!(stats.max_ns, 200);
        // Median of [100, 200] at p50: ceil(0.5 * 2) = 1, index 0 -> 100
        assert_eq!(stats.median_ns, 100);
        // P95 of [100, 200]: ceil(0.95 * 2) = 2, index 1 -> 200
        assert_eq!(stats.p95_ns, 200);
    }

    #[test]
    fn test_compute_stats_ten_samples() {
        // 10 samples: 1, 2, 3, 4, 5, 6, 7, 8, 9, 10
        let samples: Vec<u64> = (1..=10).collect();
        let stats = compute_stats(&samples);

        assert_eq!(stats.min_ns, 1);
        assert_eq!(stats.max_ns, 10);
        // Median (p50): ceil(0.5 * 10) = 5, index 4 -> 5
        assert_eq!(stats.median_ns, 5);
        // P95: ceil(0.95 * 10) = 10, index 9 -> 10
        assert_eq!(stats.p95_ns, 10);
    }

    #[test]
    fn test_compute_stats_unsorted_input() {
        // Verify that unsorted input is handled correctly
        let samples = vec![500, 100, 300, 200, 400];
        let stats = compute_stats(&samples);

        assert_eq!(stats.min_ns, 100);
        assert_eq!(stats.max_ns, 500);
        // Sorted: [100, 200, 300, 400, 500]
        // Median (p50): ceil(0.5 * 5) = 3, index 2 -> 300
        assert_eq!(stats.median_ns, 300);
        // P95: ceil(0.95 * 5) = 5, index 4 -> 500
        assert_eq!(stats.p95_ns, 500);
    }

    #[test]
    fn test_compute_stats_hundred_samples() {
        // 100 samples: 1 to 100
        let samples: Vec<u64> = (1..=100).collect();
        let stats = compute_stats(&samples);

        assert_eq!(stats.min_ns, 1);
        assert_eq!(stats.max_ns, 100);
        // Median (p50): ceil(0.5 * 100) = 50, index 49 -> 50
        assert_eq!(stats.median_ns, 50);
        // P95: ceil(0.95 * 100) = 95, index 94 -> 95
        assert_eq!(stats.p95_ns, 95);
    }

    #[test]
    fn test_percentile_edge_cases() {
        let sorted = vec![10, 20, 30, 40, 50];

        assert_eq!(percentile(&sorted, 0), 10);
        assert_eq!(percentile(&sorted, 100), 50);
    }

    #[test]
    #[should_panic(expected = "samples must not be empty")]
    fn test_compute_stats_empty_panics() {
        let samples: Vec<u64> = vec![];
        let _ = compute_stats(&samples);
    }

    #[test]
    fn test_compute_median_odd() {
        assert_eq!(compute_median(&[3, 1, 2]), 2);
        assert_eq!(compute_median(&[5, 1, 3, 2, 4]), 3);
    }

    #[test]
    fn test_compute_median_even() {
        assert_eq!(compute_median(&[1, 2, 3, 4]), 2); // (2+3)/2 = 2
        assert_eq!(compute_median(&[10, 20]), 15);
    }

    #[test]
    fn test_compute_median_single() {
        assert_eq!(compute_median(&[42]), 42);
    }

    #[test]
    #[should_panic(expected = "samples must not be empty")]
    fn test_compute_median_empty_panics() {
        let samples: Vec<u64> = vec![];
        let _ = compute_median(&samples);
    }
}
