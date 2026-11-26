use std::time::Duration;

const SLOW_SECS: u64 = 600;
const MEDIUM_SECS: u64 = 300;
const FAST_SECS: u64 = 90;
const BACKOFF_BASE_SECS: u64 = 60;
/// Cap on the exponential failure backoff.
pub const MAX_BACKOFF_SECS: u64 = 1800;

/// How long to wait before the next poll. Failure backoff takes precedence,
/// then "unchanged for two polls", then the utilisation tiers.
pub fn next_interval(five_hour_pct: Option<f64>, unchanged_polls: u32, consecutive_failures: u32) -> Duration {
    if consecutive_failures >= 1 {
        let doublings = (consecutive_failures - 1).min(16);
        let secs = (BACKOFF_BASE_SECS << doublings).min(MAX_BACKOFF_SECS);
        return Duration::from_secs(secs);
    }
    if unchanged_polls >= 2 {
        return Duration::from_secs(SLOW_SECS);
    }
    let secs = match five_hour_pct {
        None => SLOW_SECS,
        Some(p) if p < 50.0 => SLOW_SECS,
        Some(p) if p <= 80.0 => MEDIUM_SECS,
        Some(_) => FAST_SECS,
    };
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(pct: Option<f64>, unchanged: u32, failures: u32) -> u64 {
        next_interval(pct, unchanged, failures).as_secs()
    }

    #[test]
    fn tiers_by_utilization() {
        assert_eq!(secs(Some(0.0), 0, 0), 600);
        assert_eq!(secs(Some(49.9), 0, 0), 600);
        assert_eq!(secs(Some(50.0), 0, 0), 300);
        assert_eq!(secs(Some(80.0), 0, 0), 300);
        assert_eq!(secs(Some(80.1), 0, 0), 90);
        assert_eq!(secs(Some(100.0), 0, 0), 90);
    }

    #[test]
    fn unknown_pct_is_slow() {
        assert_eq!(secs(None, 0, 0), 600);
    }

    #[test]
    fn unchanged_for_two_polls_is_slow() {
        assert_eq!(secs(Some(85.0), 2, 0), 600);
        assert_eq!(secs(Some(85.0), 5, 0), 600);
        assert_eq!(secs(Some(85.0), 1, 0), 90);
        assert_eq!(secs(Some(60.0), 2, 0), 600);
    }

    #[test]
    fn failure_backoff_doubles_and_caps() {
        assert_eq!(secs(Some(10.0), 0, 1), 60);
        assert_eq!(secs(Some(10.0), 0, 2), 120);
        assert_eq!(secs(Some(10.0), 0, 3), 240);
        assert_eq!(secs(Some(10.0), 0, 5), 960);
        assert_eq!(secs(Some(10.0), 0, 6), 1800);
        assert_eq!(secs(Some(10.0), 0, 40), 1800);
        assert_eq!(secs(Some(10.0), 0, u32::MAX), 1800);
    }

    #[test]
    fn failures_take_precedence() {
        assert_eq!(secs(Some(95.0), 3, 1), 60);
        assert_eq!(secs(None, 0, 2), 120);
    }
}
