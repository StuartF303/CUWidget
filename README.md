# CUWidget

A small always-on-top desktop widget for Windows and macOS that shows Claude subscription usage: 5-hour and 7-day utilisation, live reset countdowns, run rate (%/hr) and a projected time of hitting the limit.

Status: in development. See the [design](docs/superpowers/specs/2026-11-25-claude-usage-widget-design.md) and the [implementation plan](docs/superpowers/plans/2026-11-26-claude-usage-widget.md).

## How it works

- Reads the OAuth token that Claude Code stores on the machine (read-only; never refreshed or saved by the widget).
- Polls `https://api.anthropic.com/api/oauth/usage`, a metadata endpoint. It makes no model calls and uses no tokens.
- Polls every 10 min under 50% of the 5-hour window, 5 min from 50–80%, 90 s above 80%, and backs off on errors.

The usage endpoint is undocumented and may change without notice.

## Where the token comes from

The widget reads the OAuth token Claude Code stores, read-only. These are the expected Claude Code locations and field names; they are not yet confirmed on the owner's machines.

- JSON shape: `{"claudeAiOauth": {"accessToken": "...", "expiresAt": <epoch milliseconds>}}`. A missing `expiresAt` is treated as not expired.
- Windows: the file `%USERPROFILE%\.claude\.credentials.json`.
- macOS: the Keychain item `Claude Code-credentials`, read with `security find-generic-password -s "Claude Code-credentials" -w`.
- Other OS (Linux, local compile checks only): `$HOME/.claude/.credentials.json`.

The token is never written back or logged.

## Using it

- Drag the widget anywhere to move it; its position is remembered.
- Tray icon (menu bar on macOS): hover for the 5-hour percent and reset time. Click it (Windows) to show or hide the widget. The menu has Show/Hide, Refresh now, Start at login and Quit.
- Closing the window (Alt+F4 / Cmd+W) only hides it. Quit from the tray menu exits the app.
- It starts at login only if you tick Start at login.
- Launching it a second time just brings the existing widget to the front.

## Requirements

Claude Code installed and signed in on the same machine.

## Build

Installers (`.msi`, `.dmg`) are built by GitHub Actions. To build locally you need Rust (stable), Node 22 and the Tauri 2 prerequisites, then `npm install` and `npm run tauri build`.
