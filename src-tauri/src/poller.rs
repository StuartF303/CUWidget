//! Usage polling loop and widget state. Free of Tauri so it can be tested
//! against a mock HTTP server.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::Notify;
use usage_core::credentials::CredError;
use usage_core::policy::next_interval;
use usage_core::rate::{project, run_rate, Projection, Sample};
use usage_core::usage::{parse_usage, Window};

pub const PRODUCTION_BASE: &str = "https://api.anthropic.com";
const SIGNED_OUT_SECS: u64 = 600;
const RESET_GRACE_SECS: i64 = 5;
const MAX_BACKOFF_SECS: u64 = 1800;
const SAMPLE_WINDOW_MINUTES: i64 = 30;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Status {
    Live,
    Stale { since: DateTime<Utc> },
    SignedOut,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WidgetState {
    pub status: Status,
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub rate_pct_per_hr: Option<f64>,
    pub projection: Projection,
}

/// Reads the access token fresh on every poll. Production: `read_access_token`.
pub type TokenReader = Box<dyn Fn(DateTime<Utc>) -> Result<String, CredError> + Send + Sync>;

pub struct Poller {
    base: String,
    client: reqwest::Client,
    read_token: TokenReader,
    samples: Vec<Sample>,
    samples_path: Option<PathBuf>,
    last: WidgetState,
    last_five_pct: Option<f64>,
    unchanged_polls: u32,
    consecutive_failures: u32,
    failing_since: Option<DateTime<Utc>>,
}

enum Outcome {
    Body(String),
    SignedOut,
    Failure { retry_after: Option<u64> },
}

/// Load samples from a JSON-lines file, keeping only the last 30 minutes.
/// Unreadable files and malformed lines are ignored.
pub fn load_samples(path: &Path, now: DateTime<Utc>) -> Vec<Sample> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let cutoff = now - chrono::Duration::minutes(SAMPLE_WINDOW_MINUTES);
    text.lines()
        .filter_map(|l| serde_json::from_str::<Sample>(l).ok())
        .filter(|s| s.at >= cutoff)
        .collect()
}

/// Rewrite the samples file. Errors are non-fatal and ignored.
pub fn save_samples(path: &Path, samples: &[Sample]) {
    let mut out = String::new();
    for s in samples {
        if let Ok(line) = serde_json::to_string(s) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    let _ = std::fs::write(path, out);
}

impl Poller {
    pub fn new(
        base: impl Into<String>,
        read_token: TokenReader,
        samples_path: Option<PathBuf>,
        now: DateTime<Utc>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("CUWidget/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build HTTP client");
        let samples = samples_path.as_deref().map(|p| load_samples(p, now)).unwrap_or_default();
        Poller {
            base: base.into().trim_end_matches('/').to_string(),
            client,
            read_token,
            samples,
            samples_path,
            last: empty_state(Status::Live),
            last_five_pct: None,
            unchanged_polls: 0,
            consecutive_failures: 0,
            failing_since: None,
        }
    }

    pub async fn poll_once(&mut self, now: DateTime<Utc>) -> (WidgetState, Duration) {
        let token = match (self.read_token)(now) {
            Ok(t) => t,
            Err(_) => return self.signed_out(),
        };
        match self.fetch(token).await {
            Outcome::SignedOut => self.signed_out(),
            Outcome::Body(text) => match parse_usage(&text, now) {
                Ok(snap) => self.live(snap.five_hour, snap.seven_day, now),
                Err(_) => self.failed(now, None),
            },
            Outcome::Failure { retry_after } => self.failed(now, retry_after),
        }
    }

    /// One request. The token is moved in and dropped when this returns; it is
    /// never logged or included in any error.
    async fn fetch(&self, token: String) -> Outcome {
        let resp = self
            .client
            .get(format!("{}/api/oauth/usage", self.base))
            .bearer_auth(&token)
            .header("anthropic-beta", "oauth-2025-04-20")
            .send()
            .await;
        drop(token);
        let Ok(resp) = resp else {
            return Outcome::Failure { retry_after: None };
        };
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Outcome::SignedOut;
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            return Outcome::Failure { retry_after };
        }
        if !status.is_success() {
            return Outcome::Failure { retry_after: None };
        }
        match resp.text().await {
            Ok(text) => Outcome::Body(text),
            Err(_) => Outcome::Failure { retry_after: None },
        }
    }

    fn signed_out(&mut self) -> (WidgetState, Duration) {
        self.last = empty_state(Status::SignedOut);
        self.last_five_pct = None;
        self.unchanged_polls = 0;
        (self.last.clone(), Duration::from_secs(SIGNED_OUT_SECS))
    }

    fn live(&mut self, five: Option<Window>, seven: Option<Window>, now: DateTime<Utc>) -> (WidgetState, Duration) {
        self.consecutive_failures = 0;
        self.failing_since = None;

        let pct = five.as_ref().map(|w| w.utilization);
        self.unchanged_polls = match (pct, self.last_five_pct) {
            (Some(a), Some(b)) if a == b => self.unchanged_polls.saturating_add(1),
            _ => 0,
        };
        self.last_five_pct = pct;

        if let Some(p) = pct {
            self.samples.push(Sample { at: now, pct: p });
        }
        let cutoff = now - chrono::Duration::minutes(SAMPLE_WINDOW_MINUTES);
        self.samples.retain(|s| s.at >= cutoff);
        if let Some(path) = &self.samples_path {
            save_samples(path, &self.samples);
        }

        let rate = run_rate(&self.samples, now);
        let projection = match &five {
            Some(w) => project(w.utilization, rate, now, w.resets_at),
            None => Projection::Unknown,
        };

        let mut wait = next_interval(pct, self.unchanged_polls, 0);
        if let Some(reset) = five.as_ref().and_then(|w| w.resets_at) {
            let until = (reset - now).num_milliseconds();
            if until > 0 {
                let target = Duration::from_millis(until as u64) + Duration::from_secs(RESET_GRACE_SECS as u64);
                wait = wait.min(target);
            }
        }

        self.last = WidgetState { status: Status::Live, five_hour: five, seven_day: seven, rate_pct_per_hr: rate, projection };
        (self.last.clone(), wait)
    }

    fn failed(&mut self, now: DateTime<Utc>, retry_after: Option<u64>) -> (WidgetState, Duration) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let since = *self.failing_since.get_or_insert(now);
        let mut wait = next_interval(self.last_five_pct, self.unchanged_polls, self.consecutive_failures);
        if let Some(secs) = retry_after {
            wait = wait.max(Duration::from_secs(secs)).min(Duration::from_secs(MAX_BACKOFF_SECS));
        }
        let mut state = self.last.clone();
        state.status = Status::Stale { since };
        self.last = state.clone();
        (state, wait)
    }
}

fn empty_state(status: Status) -> WidgetState {
    WidgetState { status, five_hour: None, seven_day: None, rate_pct_per_hr: None, projection: Projection::Unknown }
}

const WAIT_TICK: Duration = Duration::from_secs(15);

/// Wait until the wall-clock `deadline` or until `refresh` fires. Sleeps in
/// short ticks and re-reads the wall clock each time, because the monotonic
/// timer does not advance while the system is suspended: after wake the
/// deadline is seen to have passed within one tick.
async fn wait_until<C: Fn() -> DateTime<Utc>>(clock: C, deadline: DateTime<Utc>, tick: Duration, refresh: &Notify) {
    loop {
        let remaining = deadline - clock();
        if remaining <= chrono::Duration::zero() {
            return;
        }
        let step = remaining.to_std().map_or(tick, |r| r.min(tick));
        tokio::select! {
            _ = tokio::time::sleep(step) => {}
            _ = refresh.notified() => return,
        }
    }
}

/// Poll, emit, then wait for the next poll or for `refresh` to fire.
pub async fn run<F: Fn(WidgetState)>(mut poller: Poller, refresh: Arc<Notify>, emit: F) {
    loop {
        let (state, wait) = poller.poll_once(Utc::now()).await;
        emit(state);
        let deadline = Utc::now() + chrono::Duration::from_std(wait).unwrap_or(chrono::Duration::seconds(MAX_BACKOFF_SECS as i64));
        wait_until(Utc::now, deadline, WAIT_TICK, &refresh).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as CD, TimeZone};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 22, 4, 0, 0).unwrap()
    }

    fn body(five: Option<(f64, DateTime<Utc>)>, seven: f64) -> String {
        let five = match five {
            Some((u, r)) => format!(r#"{{"utilization":{u},"resets_at":"{}"}}"#, r.to_rfc3339()),
            None => "null".into(),
        };
        format!(
            r#"{{"five_hour":{five},"seven_day":{{"utilization":{seven},"resets_at":"2026-01-25T09:00:00Z"}}}}"#
        )
    }

    fn ok_token() -> TokenReader {
        Box::new(|_| Ok("secret-token".to_string()))
    }

    fn poller(server: &MockServer) -> Poller {
        Poller::new(server.uri(), ok_token(), None, now())
    }

    async fn mount_ok(server: &MockServer, b: String) {
        Mock::given(method("GET")).and(path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_string(b))
            .mount(server).await;
    }

    #[tokio::test]
    async fn ok_is_live_with_headers_and_low_interval() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/oauth/usage"))
            .and(header("authorization", "Bearer secret-token"))
            .and(header("anthropic-beta", "oauth-2025-04-20"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body(Some((20.0, now() + CD::hours(3))), 40.0)))
            .expect(1)
            .mount(&server).await;
        let (state, wait) = poller(&server).poll_once(now()).await;
        assert_eq!(state.status, Status::Live);
        assert_eq!(state.five_hour.unwrap().utilization, 20.0);
        assert_eq!(state.seven_day.unwrap().utilization, 40.0);
        assert_eq!(wait, Duration::from_secs(600));
    }

    #[tokio::test]
    async fn user_agent_is_sent() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(header("user-agent", concat!("CUWidget/", env!("CARGO_PKG_VERSION"))))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1).mount(&server).await;
        let (state, _) = poller(&server).poll_once(now()).await;
        assert_eq!(state.status, Status::Live);
    }

    #[tokio::test]
    async fn high_utilisation_polls_every_90s() {
        let server = MockServer::start().await;
        mount_ok(&server, body(Some((90.0, now() + CD::hours(3))), 40.0)).await;
        let (_, wait) = poller(&server).poll_once(now()).await;
        assert_eq!(wait, Duration::from_secs(90));
    }

    #[tokio::test]
    async fn unauthorised_is_signed_out_without_backoff() {
        for code in [401u16, 403] {
            let server = MockServer::start().await;
            Mock::given(method("GET")).respond_with(ResponseTemplate::new(code)).mount(&server).await;
            let mut p = poller(&server);
            let (state, wait) = p.poll_once(now()).await;
            assert_eq!(state.status, Status::SignedOut);
            assert_eq!(state.five_hour, None);
            assert_eq!(wait, Duration::from_secs(600));
            assert_eq!(p.consecutive_failures, 0);
        }
    }

    #[tokio::test]
    async fn rate_limit_then_ok_keeps_values_then_recovers() {
        let server = MockServer::start().await;
        let b = body(Some((90.0, now() + CD::hours(3))), 40.0);
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_string(b))
            .up_to_n_times(1).mount(&server).await;
        let mut p = poller(&server);
        let (live, _) = p.poll_once(now()).await;
        server.reset().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(429)).up_to_n_times(1).mount(&server).await;
        mount_ok(&server, body(Some((91.0, now() + CD::hours(3))), 41.0)).await;

        let t1 = now() + CD::seconds(90);
        let (stale, wait) = p.poll_once(t1).await;
        assert_eq!(stale.status, Status::Stale { since: t1 });
        assert_eq!(stale.five_hour, live.five_hour);
        assert_eq!(stale.seven_day, live.seven_day);
        assert_eq!(wait, Duration::from_secs(60));
        assert_eq!(p.consecutive_failures, 1);

        let (again, _) = p.poll_once(t1 + CD::seconds(60)).await;
        assert_eq!(again.status, Status::Live);
        assert_eq!(again.five_hour.unwrap().utilization, 91.0);
        assert_eq!(p.consecutive_failures, 0);
        assert_eq!(p.failing_since, None);
    }

    #[tokio::test]
    async fn stale_since_is_first_failure_of_run() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(503)).mount(&server).await;
        let mut p = poller(&server);
        let (_, w1) = p.poll_once(now()).await;
        let (s2, w2) = p.poll_once(now() + CD::seconds(60)).await;
        assert_eq!(s2.status, Status::Stale { since: now() });
        assert_eq!((w1.as_secs(), w2.as_secs()), (60, 120));
    }

    #[tokio::test]
    async fn retry_after_extends_sleep_and_is_capped() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "300"))
            .up_to_n_times(1).mount(&server).await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "99999"))
            .mount(&server).await;
        let mut p = poller(&server);
        let (_, wait) = p.poll_once(now()).await;
        assert_eq!(wait, Duration::from_secs(300));
        let (_, wait) = p.poll_once(now()).await;
        assert_eq!(wait, Duration::from_secs(1800));
    }

    #[tokio::test]
    async fn credential_errors_are_signed_out_with_no_request() {
        for err in [CredError::NotFound, CredError::Expired] {
            let server = MockServer::start().await;
            let e = err.clone();
            let mut p = Poller::new(server.uri(), Box::new(move |_| Err(e.clone())), None, now());
            let (state, wait) = p.poll_once(now()).await;
            assert_eq!(state.status, Status::SignedOut);
            assert_eq!(wait, Duration::from_secs(600));
            assert!(server.received_requests().await.unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn null_five_hour_is_live_with_none() {
        let server = MockServer::start().await;
        mount_ok(&server, body(None, 12.0)).await;
        let (state, wait) = poller(&server).poll_once(now()).await;
        assert_eq!(state.status, Status::Live);
        assert_eq!(state.five_hour, None);
        assert_eq!(state.seven_day.unwrap().utilization, 12.0);
        assert_eq!(state.projection, Projection::Unknown);
        assert_eq!(wait, Duration::from_secs(600));
    }

    #[tokio::test]
    async fn unparsable_body_is_stale() {
        let server = MockServer::start().await;
        mount_ok(&server, "not json".into()).await;
        let (state, wait) = poller(&server).poll_once(now()).await;
        assert_eq!(state.status, Status::Stale { since: now() });
        assert_eq!(wait, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn unchanged_polls_slow_down() {
        let server = MockServer::start().await;
        mount_ok(&server, body(Some((85.0, now() + CD::hours(3))), 40.0)).await;
        let mut p = poller(&server);
        let mut waits = Vec::new();
        for i in 0..3 {
            let (_, w) = p.poll_once(now() + CD::seconds(90 * i)).await;
            waits.push(w.as_secs());
        }
        assert_eq!(waits, vec![90, 90, 600]);
        assert_eq!(p.unchanged_polls, 2);
    }

    #[tokio::test]
    async fn imminent_reset_shortens_sleep() {
        let server = MockServer::start().await;
        mount_ok(&server, body(Some((90.0, now() + CD::seconds(30))), 40.0)).await;
        let (_, wait) = poller(&server).poll_once(now()).await;
        assert_eq!(wait, Duration::from_secs(35));
    }

    #[tokio::test]
    async fn past_reset_does_not_shorten_sleep() {
        let server = MockServer::start().await;
        mount_ok(&server, body(Some((90.0, now() - CD::seconds(30))), 40.0)).await;
        let (_, wait) = poller(&server).poll_once(now()).await;
        assert_eq!(wait, Duration::from_secs(90));
    }

    #[tokio::test]
    async fn rate_and_projection_from_samples() {
        let server = MockServer::start().await;
        mount_ok(&server, body(Some((46.0, now() + CD::hours(5))), 40.0)).await;
        let mut p = poller(&server);
        p.samples.push(Sample { at: now() - CD::minutes(30), pct: 40.0 });
        let (state, _) = p.poll_once(now()).await;
        assert_eq!(state.rate_pct_per_hr, Some(12.0));
        assert_eq!(state.projection, Projection::HitsLimitAt { at: now() + CD::minutes(270) });
    }

    #[test]
    fn samples_round_trip_drops_old() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("samples.jsonl");
        let keep = Sample { at: now() - CD::minutes(10), pct: 5.0 };
        let old = Sample { at: now() - CD::minutes(31), pct: 4.0 };
        save_samples(&file, &[old, keep.clone()]);
        assert_eq!(load_samples(&file, now()), vec![keep]);
        assert!(load_samples(&dir.path().join("missing"), now()).is_empty());
    }

    #[tokio::test]
    async fn poll_persists_trimmed_samples() {
        let server = MockServer::start().await;
        mount_ok(&server, body(Some((46.0, now() + CD::hours(5))), 40.0)).await;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("samples.jsonl");
        save_samples(&file, &[Sample { at: now() - CD::minutes(20), pct: 40.0 }]);
        let mut p = Poller::new(server.uri(), ok_token(), Some(file.clone()), now());
        p.poll_once(now()).await;
        assert_eq!(load_samples(&file, now()).len(), 2);
        p.poll_once(now() + CD::minutes(15)).await;
        // the 20-minute-old sample is now 35 minutes old and is dropped
        assert_eq!(load_samples(&file, now() + CD::minutes(15)).len(), 2);
        assert_eq!(std::fs::read_to_string(&file).unwrap().lines().count(), 2);
    }

    #[tokio::test]
    async fn wait_returns_after_wall_clock_jump_despite_little_monotonic_time() {
        use std::sync::atomic::{AtomicI64, Ordering};
        let jump = AtomicI64::new(0);
        let start = now();
        let clock = || start + CD::seconds(jump.load(Ordering::SeqCst));
        let refresh = Notify::new();
        let deadline = start + CD::hours(1);
        let tick = Duration::from_millis(5);

        // No jump: still waiting.
        let waiting = tokio::time::timeout(Duration::from_millis(60), wait_until(clock, deadline, tick, &refresh)).await;
        assert!(waiting.is_err());

        // Simulated suspend: wall clock jumps an hour while only milliseconds pass.
        let began = std::time::Instant::now();
        let (_, done) = tokio::join!(
            async { tokio::time::sleep(Duration::from_millis(20)).await; jump.store(3600, Ordering::SeqCst); },
            tokio::time::timeout(Duration::from_secs(2), wait_until(clock, deadline, tick, &refresh)),
        );
        assert!(done.is_ok());
        assert!(began.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn wait_is_woken_early_by_refresh() {
        let refresh = Notify::new();
        refresh.notify_one();
        let done = tokio::time::timeout(
            Duration::from_secs(2),
            wait_until(now, now() + CD::hours(1), Duration::from_secs(15), &refresh),
        )
        .await;
        assert!(done.is_ok());
    }

    #[tokio::test]
    async fn wait_returns_immediately_when_deadline_passed() {
        let refresh = Notify::new();
        let done = tokio::time::timeout(
            Duration::from_secs(2),
            wait_until(now, now() - CD::seconds(1), Duration::from_secs(15), &refresh),
        )
        .await;
        assert!(done.is_ok());
    }

    #[test]
    fn status_serialises_tagged() {
        let s = serde_json::to_string(&Status::Stale { since: now() }).unwrap();
        assert_eq!(s, r#"{"kind":"stale","since":"2026-01-22T04:00:00Z"}"#);
        assert_eq!(serde_json::to_string(&Status::Live).unwrap(), r#"{"kind":"live"}"#);
        assert_eq!(serde_json::to_string(&Status::SignedOut).unwrap(), r#"{"kind":"signed_out"}"#);
    }
}
