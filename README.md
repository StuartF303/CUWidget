# CUWidget

A small always-on-top desktop widget for Windows and macOS that shows Claude subscription usage: 5-hour and 7-day utilisation, live reset countdowns, run rate (%/hr) and a projected time of hitting the limit.

Status: in development. See the [design](docs/superpowers/specs/2026-11-25-claude-usage-widget-design.md) and the [implementation plan](docs/superpowers/plans/2026-11-26-claude-usage-widget.md).

## How it works

- Reads the OAuth token that Claude Code stores on the machine (read-only; never refreshed or saved by the widget).
- Polls `https://api.anthropic.com/api/oauth/usage`, a metadata endpoint. It makes no model calls and uses no tokens.
- Polls every 10 min under 50% of the 5-hour window, 5 min from 50–80%, 90 s above 80%, and backs off on errors.

The usage endpoint is undocumented and may change without notice.

## Requirements

Claude Code installed and signed in on the same machine.

## Build

Installers (`.msi`, `.dmg`) are built by GitHub Actions. To build locally you need Rust (stable), Node 22 and the Tauri 2 prerequisites, then `npm install` and `npm run tauri build`.
