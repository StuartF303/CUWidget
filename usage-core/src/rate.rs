use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub at: DateTime<Utc>,
    pub pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Projection {
    HitsLimitAt { at: DateTime<Utc> },
    OkUntilReset,
    LimitReached,
    Unknown,
}

/// Length of the run-rate window.
pub const WINDOW_MINUTES: i64 = 30;
/// Minimum span the usable samples must cover before a rate is reported.
const MIN_SPAN_MINUTES: i64 = 5;

/// Run rate in percentage points per hour over the last 30 minutes, using only
/// samples after the most recent drop in utilisation (a drop means the usage
/// window reset). Never negative; `None` with fewer than two usable samples or
/// when they span less than 5 minutes.
pub fn run_rate(samples: &[Sample], now: DateTime<Utc>) -> Option<f64> {
    let cutoff = now - Duration::minutes(WINDOW_MINUTES);
    let mut recent: Vec<&Sample> = samples.iter().filter(|s| s.at >= cutoff && s.at <= now).collect();
    recent.sort_by_key(|s| s.at);

    let start = recent
        .windows(2)
        .rposition(|w| w[1].pct < w[0].pct)
        .map_or(0, |i| i + 1);
    let usable = &recent[start..];
    let (first, last) = (usable.first()?, usable.last()?);

    let elapsed_secs = (last.at - first.at).num_milliseconds() as f64 / 1000.0;
    if usable.len() < 2 || elapsed_secs < (MIN_SPAN_MINUTES * 60) as f64 {
        return None;
    }
    Some(((last.pct - first.pct) / (elapsed_secs / 3600.0)).max(0.0))
}

/// Project when the limit will be hit at the current rate.
pub fn project(pct: f64, rate: Option<f64>, now: DateTime<Utc>, resets_at: Option<DateTime<Utc>>) -> Projection {
    if pct >= 100.0 {
        return Projection::LimitReached;
    }
    let Some(rate) = rate else {
        return Projection::Unknown;
    };
    if rate <= 0.0 {
        return Projection::OkUntilReset;
    }
    let hours = (100.0 - pct) / rate;
    let hit = Duration::try_milliseconds((hours * 3_600_000.0).round() as i64)
        .and_then(|d| now.checked_add_signed(d));
    match hit {
        // Too far away to represent: it will not happen before any reset.
        None => Projection::OkUntilReset,
        Some(hit) => match resets_at {
            Some(reset) if hit >= reset => Projection::OkUntilReset,
            _ => Projection::HitsLimitAt { at: hit },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 22, 4, 0, 0).unwrap()
    }
    fn s(mins_ago: i64, pct: f64) -> Sample {
        Sample { at: now() - Duration::minutes(mins_ago), pct }
    }

    #[test]
    fn rate_over_thirty_minutes() {
        assert_eq!(run_rate(&[s(30, 40.0), s(0, 46.0)], now()), Some(12.0));
    }

    #[test]
    fn rate_needs_two_samples() {
        assert_eq!(run_rate(&[], now()), None);
        assert_eq!(run_rate(&[s(5, 40.0)], now()), None);
    }

    #[test]
    fn rate_zero_elapsed_is_none() {
        assert_eq!(run_rate(&[s(5, 40.0), s(5, 41.0)], now()), None);
    }

    #[test]
    fn old_samples_ignored() {
        // 31 minutes ago is outside the window, leaving only one usable sample.
        assert_eq!(run_rate(&[s(31, 10.0), s(10, 40.0)], now()), None);
        // With the old sample dropped, rate comes from the two inside the window.
        assert_eq!(run_rate(&[s(45, 0.0), s(30, 40.0), s(0, 46.0)], now()), Some(12.0));
        assert_eq!(run_rate(&[s(45, 0.0), s(20, 40.0), s(0, 46.0)], now()), Some(18.0));
    }

    #[test]
    fn rate_uses_samples_after_latest_reset() {
        // 90 -> 2 is a reset; only 2 -> 5 over 10 minutes counts: 3 / (1/6 h) = 18.
        assert_eq!(run_rate(&[s(25, 90.0), s(10, 2.0), s(0, 5.0)], now()), Some(18.0));
    }

    #[test]
    fn reset_leaving_one_sample_is_none() {
        assert_eq!(run_rate(&[s(10, 90.0), s(0, 2.0)], now()), None);
    }

    #[test]
    fn flat_is_zero() {
        assert_eq!(run_rate(&[s(20, 40.0), s(0, 40.0)], now()), Some(0.0));
    }

    #[test]
    fn project_hits_limit_before_reset() {
        let resets = now() + Duration::hours(5);
        assert_eq!(
            project(58.0, Some(12.0), now(), Some(resets)),
            Projection::HitsLimitAt { at: now() + Duration::minutes(210) }
        );
    }

    #[test]
    fn project_ok_when_reset_comes_first() {
        let resets = now() + Duration::hours(1);
        assert_eq!(project(58.0, Some(12.0), now(), Some(resets)), Projection::OkUntilReset);
    }

    #[test]
    fn project_hit_exactly_at_reset_is_ok() {
        let resets = now() + Duration::minutes(210);
        assert_eq!(project(58.0, Some(12.0), now(), Some(resets)), Projection::OkUntilReset);
    }

    #[test]
    fn project_without_reset_time() {
        assert_eq!(
            project(58.0, Some(12.0), now(), None),
            Projection::HitsLimitAt { at: now() + Duration::minutes(210) }
        );
    }

    #[test]
    fn project_zero_rate_ok_and_no_rate_unknown() {
        assert_eq!(project(58.0, Some(0.0), now(), None), Projection::OkUntilReset);
        assert_eq!(project(58.0, None, now(), None), Projection::Unknown);
    }

    #[test]
    fn project_already_at_limit() {
        assert_eq!(project(100.0, Some(5.0), now(), None), Projection::LimitReached);
        assert_eq!(project(104.5, Some(5.0), now(), None), Projection::LimitReached);
        // Checked first, whatever the rate.
        assert_eq!(project(100.0, Some(0.0), now(), None), Projection::LimitReached);
        assert_eq!(project(100.0, Some(-1.0), now(), None), Projection::LimitReached);
        assert_eq!(project(100.0, None, now(), None), Projection::LimitReached);
        assert_eq!(project(99.9, None, now(), None), Projection::Unknown);
    }

    #[test]
    fn rate_needs_five_minute_span() {
        assert_eq!(run_rate(&[s(4, 40.0), s(0, 45.0)], now()), None);
        assert_eq!(run_rate(&[s(5, 40.0), s(0, 41.0)], now()), Some(12.0));
        // The span is measured after the latest reset.
        assert_eq!(run_rate(&[s(20, 90.0), s(4, 2.0), s(0, 5.0)], now()), None);
    }

    #[test]
    fn projection_serialises_tagged() {
        let j = serde_json::to_string(&Projection::HitsLimitAt { at: now() }).unwrap();
        assert_eq!(j, r#"{"kind":"hits_limit_at","at":"2026-01-22T04:00:00Z"}"#);
        assert_eq!(serde_json::to_string(&Projection::OkUntilReset).unwrap(), r#"{"kind":"ok_until_reset"}"#);
        assert_eq!(serde_json::to_string(&Projection::LimitReached).unwrap(), r#"{"kind":"limit_reached"}"#);
    }
}
