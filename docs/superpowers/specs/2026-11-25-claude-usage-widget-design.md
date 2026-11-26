# Claude Usage Widget: short design

Date: 2026-11-25 · Status: approved 2026-11-26 · Size: small project

## Intent

A small always-visible desktop widget on Windows and macOS showing how much of Stuart's Claude subscription allowance is used, when each window resets, and how fast it is being consumed, so he can pace work and not hit a limit by surprise. First trial of the wider Desktop Widgets project.

## Goals

- Show 5-hour and 7-day utilisation (%), each with a live reset countdown.
- Show run rate (%/hr) and a projected time of hitting 100% if the current rate holds.
- Poll adaptively: slow when usage is low, faster near the limit.
- Cost zero tokens to run.

## Non-goals

- iOS (parked: the phone has no token; needs a relay).
- Absolute token counts (the subscription endpoint only gives percentages).
- API Console / pay-as-you-go spend.
- Code signing and notarisation for distribution.

## Anti-goals

- Never make a model call.
- Never refresh or rewrite Claude Code's OAuth token (risk of logging Claude Code out).
- Never store the token; hold it in memory only for the duration of each request.
- Never hammer the endpoint: honour 429s.

## Approaches considered

1. **Tauri 2 (chosen).** Rust core + OS webview. Small, good-looking frameless widget, tray built in, path to iOS later. Chosen because Stuart does not intend to write the code himself, so UI quality and footprint outweigh language familiarity.
2. **Avalonia (C#).** One codebase, Stuart's language, easier for him to maintain. Not chosen for the reason above.
3. **Native per platform (WinUI + SwiftUI/WidgetKit).** True OS widgets, but two codebases and OS-controlled refresh that fights 1–2 minute polling.

## Design

**Data source.** `GET https://api.anthropic.com/api/oauth/usage` with `Authorization: Bearer <token>` and `anthropic-beta: oauth-2025-04-20`. Returns `five_hour` and `seven_day` objects, each with `utilization` (percent) and `resets_at` (ISO 8601). Other fields (per-model windows, `extra_usage`) may be present or null; parse leniently and ignore unknowns. The endpoint is undocumented and may change.

**Credentials.** Read Claude Code's stored OAuth access token fresh on every poll: the macOS Keychain entry on macOS, `~/.claude/.credentials.json` on Windows. Exact entry and field names to be confirmed on Stuart's machines as the first build step. If the token is missing or expired, show a "sign in / open Claude Code" state and keep retrying at the slow interval.

**Polling (Rust side).**

| 5-hour utilisation | Interval |
|---|---|
| under 50% | 10 min |
| 50–80% | 5 min |
| over 80% | 90 s |

- If the value has not moved for two polls, fall back to 10 min until it moves.
- On 429 or network error: exponential backoff, capped at 30 min; widget shows "stale since HH:MM".
- When `resets_at` passes, poll once immediately.
- Countdowns tick locally every second from `resets_at`; no network involved.

**Run rate.** Keep recent samples (timestamp, 5-hour %) in memory and in a small local file. Rate = change over the last 30 minutes, in %/hr. Projection = time at which 100% is reached at that rate; shown only if it lands before the reset, otherwise "OK until reset".

**Widget.** Frameless, always-on-top, draggable, position remembered. Roughly 260 × 130 px:

```
 Claude usage                 ● live
 5h  ███████░░░░░  58%   resets 1h 42m
 7d  ███░░░░░░░░░  23%   resets 4d 6h
 12 %/hr  ·  hits limit ~19:55
```

Bars turn amber at 50% and red at 80%. Tray / menu-bar icon shows the 5-hour % and offers Show/Hide, Refresh now, Start at login, Quit.

## Calls made by Claude (correct any)

- Thresholds 50/80% and intervals 10 min / 5 min / 90 s.
- 30-minute window for the run-rate calculation.
- Polling tiers driven by the 5-hour window only; the 7-day bar turns red on its own threshold but does not speed polling.
- Read-only on credentials, with a "sign in" state when the token has expired.
- Windows and macOS only; no Linux build.

## Left to the builder

- Visual styling details, icon, light/dark handling.
- Sample file format and location.
- Project layout and test approach.
