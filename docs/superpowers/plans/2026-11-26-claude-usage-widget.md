# Claude Usage Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tauri 2 tray app for Windows and macOS showing Claude subscription utilisation, reset countdowns and run rate, polling adaptively at zero token cost.

**Architecture:** All logic lives in a plain Rust library crate (`usage-core`) with no Tauri dependency, so it is unit-tested on any OS. A thin Tauri shell (`src-tauri`) owns the HTTP call, timer, tray and window, and pushes one `WidgetState` event to a static HTML/TypeScript front end that only renders.

**Tech Stack:** Rust (stable), Tauri 2, `reqwest` (rustls), `serde`, `chrono`, `tauri-plugin-autostart`, `tauri-plugin-window-state`; front end vanilla TypeScript + Vite, no framework. Installers built by GitHub Actions (`tauri-apps/tauri-action`) on `windows-latest` and `macos-latest`.

**Spec:** `docs/superpowers/specs/2026-11-25-claude-usage-widget-design.md`

## Global Constraints

- Endpoint: `GET https://api.anthropic.com/api/oauth/usage`, headers `Authorization: Bearer <token>`, `anthropic-beta: oauth-2025-04-20`. No other Anthropic endpoint is ever called.
- Never make a model call. Never refresh, write or persist the OAuth token; read it fresh per poll and drop it after the request.
- Token never crosses into the webview; the front end receives `WidgetState` only.
- Polling tiers on 5-hour utilisation: under 50% → 600 s; 50–80% → 300 s; over 80% → 90 s. Unchanged for two polls → 600 s. Failure backoff doubles from 60 s, capped at 1800 s.
- Run-rate window: 30 minutes. Bar colours: amber at 50%, red at 80%.
- Windows and macOS only. Widget about 260 × 130 px, frameless, always on top.

## Review Focus

1. A window object is `null` or missing (`five_hour: null`) → widget shows "—" for that row, does not crash or stop polling. (Task 2)
2. Utilisation drops between samples because the window reset → run rate is computed only from samples after the drop, never negative. (Task 4)
3. Machine sleeps and wakes with `resets_at` in the past and stale samples → poll immediately, discard samples older than 30 min. (Tasks 4, 5)
4. Credentials missing, unreadable, malformed JSON, or token expired / HTTP 401 → `SignedOut` state, retry at 600 s, no backoff escalation. (Tasks 3, 5)
5. `utilization` above 100 or `resets_at` unparseable → clamp bar to 100%, show the raw number, show "reset time unknown". (Tasks 2, 6)

---

## File structure

```
usage-core/src/{lib.rs, usage.rs, credentials.rs, policy.rs, rate.rs}
src-tauri/src/{main.rs, poller.rs, tray.rs}
src-tauri/tauri.conf.json
ui/{index.html, main.ts, style.css}
.github/workflows/build.yml
```

### Task 1: Repo scaffold and CI build

**Files:** workspace `Cargo.toml`, `usage-core/`, `src-tauri/`, `ui/`, `.github/workflows/build.yml`.

- [ ] Scaffold a Tauri 2 app (identifier `net.stuartfraser.claudeusage`, product name `Claude Usage`) with an empty `usage-core` crate in the same Cargo workspace.
- [ ] Add the workflow: on push to `main` and on tags `v*`, run `cargo test -p usage-core`, then `tauri-action` for Windows (`.msi`) and macOS universal (`.dmg`), uploading artifacts.
- [ ] Verify: workflow green, both installers downloadable. Commit.

### Task 2: Usage response parsing

**Files:** `usage-core/src/usage.rs`

**Produces:**
```rust
pub struct Window { pub utilization: f64, pub resets_at: Option<DateTime<Utc>> }
pub struct UsageSnapshot { pub five_hour: Option<Window>, pub seven_day: Option<Window>, pub fetched_at: DateTime<Utc> }
pub fn parse_usage(json: &str, fetched_at: DateTime<Utc>) -> Result<UsageSnapshot, ParseError>
```

- [ ] Tests first: full response (`five_hour` 8.0 / `2026-01-22T09:00:00Z`, `seven_day` 77.0); extra unknown fields ignored; `five_hour: null` → `None`; `resets_at: null` → `None`; `resets_at` unparseable string → `None` (not an error); `utilization: 104.5` preserved; non-JSON → `Err`.
- [ ] Implement, `cargo test -p usage-core usage`, commit.

### Task 3: Credential reading

**Files:** `usage-core/src/credentials.rs`

**Produces:**
```rust
pub enum CredError { NotFound, Malformed, Expired }
pub fn parse_credentials(json: &str, now: DateTime<Utc>) -> Result<String, CredError>
pub fn read_access_token(now: DateTime<Utc>) -> Result<String, CredError>
```

- [ ] **Confirm on the real machines first** where Claude Code keeps its token and the JSON field names; expected `claudeAiOauth.accessToken` / `expiresAt` (epoch ms), in `%USERPROFILE%\.claude\.credentials.json` on Windows and the `Claude Code-credentials` Keychain item on macOS (read via `security find-generic-password -s "Claude Code-credentials" -w`). Record what was found in the repo README. If it differs, fix the names below before coding.
- [ ] Tests for `parse_credentials`: valid → token; missing key → `Malformed`; `expiresAt` in the past → `Expired`; empty string → `Malformed`.
- [ ] Implement `read_access_token` per OS behind `cfg(target_os)`. Commit.

### Task 4: Polling policy and run rate

**Files:** `usage-core/src/policy.rs`, `usage-core/src/rate.rs`

**Produces:**
```rust
pub fn next_interval(five_hour_pct: Option<f64>, unchanged_polls: u32, consecutive_failures: u32) -> Duration
pub struct Sample { pub at: DateTime<Utc>, pub pct: f64 }
pub fn run_rate(samples: &[Sample], now: DateTime<Utc>) -> Option<f64>   // %/hr
pub enum Projection { HitsLimitAt(DateTime<Utc>), OkUntilReset, Unknown }
pub fn project(pct: f64, rate: Option<f64>, now: DateTime<Utc>, resets_at: Option<DateTime<Utc>>) -> Projection
```

- [ ] Tests for `next_interval`: 49.9 → 600 s; 50.0 → 300 s; 80.0 → 300 s; 80.1 → 90 s; 85 with `unchanged_polls` 2 → 600 s; failures 1, 2, 6 → 60 s, 120 s, 1800 s; `None` → 600 s.
- [ ] Tests for `run_rate`: 40% → 46% over 30 min → 12.0; one sample → `None`; samples older than 30 min ignored; 90% then 2% then 5% (reset) → rate from the last two only; flat → 0.0.
- [ ] Tests for `project`: 58% at 12 %/hr with reset in 5 h → `HitsLimitAt(now + 3.5 h)`; same with reset in 1 h → `OkUntilReset`; rate 0 or `None` → `OkUntilReset` / `Unknown` respectively.
- [ ] Implement, test, commit.

### Task 5: Poller and state

**Files:** `src-tauri/src/poller.rs`, `src-tauri/src/main.rs`

**Consumes:** everything from Tasks 2–4.

**Produces:** Tauri event `usage-update` carrying
```rust
pub enum Status { Live, Stale { since: DateTime<Utc> }, SignedOut }
pub struct WidgetState { pub status: Status, pub five_hour: Option<Window>, pub seven_day: Option<Window>, pub rate_pct_per_hr: Option<f64>, pub projection: Projection }
```
and command `refresh_now()`.

- [ ] Loop: read token → GET (10 s timeout) → parse → append sample → compute → emit → sleep `next_interval`. 401 or `CredError` → `SignedOut`, sleep 600 s. 429 / network / 5xx → `Stale`, keep last values, increment failures. Wake early when `resets_at` passes or `refresh_now` is called.
- [ ] Samples persisted as JSON lines in the app data dir, trimmed to the last 30 min on load.
- [ ] Test the loop against a local mock HTTP server for: 200, 401, 429 then 200. Commit.

### Task 6: Widget UI

**Files:** `ui/index.html`, `ui/main.ts`, `ui/style.css`

**Consumes:** `usage-update`, `refresh_now`.

- [ ] Render per the spec sketch: two rows (label, bar, %, "resets 1h 42m"), a footer line "12 %/hr · hits limit ~19:55" or "OK until reset", and a status dot (live / stale since HH:MM / sign in to Claude Code).
- [ ] Countdown re-renders every second from `resets_at` with no backend call. Bar width clamps at 100%. Missing window → "—". Missing `resets_at` → "reset time unknown".
- [ ] Follows OS light/dark. Whole surface is a drag region. Commit.

### Task 7: Tray, window behaviour, release

**Files:** `src-tauri/src/tray.rs`, `src-tauri/tauri.conf.json`

- [ ] Window: 260 × 130, no decorations, transparent, always on top, skip taskbar, position restored by `tauri-plugin-window-state`.
- [ ] Tray / menu bar: tooltip "5h NN% · resets Hh Mm"; menu Show/Hide, Refresh now, Start at login (toggle, `tauri-plugin-autostart`), Quit. macOS: no Dock icon.
- [ ] Tag `v0.1.0`, confirm CI produces both installers, install on Windows and check each Review Focus item by hand. Commit.
