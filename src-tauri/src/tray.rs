//! System tray / menu-bar icon: tooltip text, menu and window toggling.

use chrono::{Local, TimeZone};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::controls::{self, TrayChecks, MAIN_WINDOW, OPEN_SETTINGS_EVENT};
use crate::poller::{Status, WidgetState};

const TRAY_ID: &str = "main";
const TOOLTIP_INITIAL: &str = "Claude usage";

/// Rounded 5-hour utilisation, if the state has a 5-hour window.
fn five_hour_percent(state: &WidgetState) -> Option<i64> {
    state.five_hour.as_ref().map(|w| w.utilization.round() as i64)
}

/// Tooltip text for the tray icon. Reset time is an absolute local HH:MM so it
/// cannot go stale between polls.
pub fn tooltip_text<Tz>(state: &WidgetState, tz: &Tz) -> String
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    if state.status == Status::SignedOut {
        return "Claude usage: open Claude Code to sign in".into();
    }
    let Some(pct) = five_hour_percent(state) else {
        return "Claude usage: no data".into();
    };
    let mut text = format!("5h {pct}%");
    if let Some(at) = state.five_hour.as_ref().and_then(|w| w.resets_at) {
        text.push_str(&format!(" · resets {}", at.with_timezone(tz).format("%H:%M")));
    }
    if matches!(state.status, Status::Stale { .. }) {
        text.push_str(" (stale)");
    }
    text
}

/// Menu-bar title: the rounded 5-hour percent, or empty when there is none.
pub fn title_text(state: &WidgetState) -> String {
    five_hour_percent(state)
        .map(|p| format!("{p}%"))
        .unwrap_or_default()
}

/// Called with each newly produced state. Failures are ignored: the tray is
/// cosmetic and must never disturb polling.
pub fn update(app: &AppHandle, state: &WidgetState) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let _ = tray.set_tooltip(Some(tooltip_text(state, &Local)));
    if cfg!(target_os = "macos") {
        let _ = tray.set_title(Some(title_text(state)));
    }
}

fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.hide();
    }
}

/// Show, unminimise and focus: brings a covered widget to the front. Never
/// depends on the window's current focus.
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn open_settings(app: &AppHandle) {
    show_main_window(app);
    let _ = app.emit(OPEN_SETTINGS_EVENT, ());
}

/// Build the tray icon and its menu. `refresh` fires the poller's `Notify`.
pub fn build(app: &AppHandle, refresh: impl Fn() + Send + Sync + 'static) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
    let refresh_item = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings\u{2026}", true, None::<&str>)?;
    let view = controls::current(app);
    let on_top = CheckMenuItem::with_id(app, "always_on_top", "Always on top", true, view.always_on_top, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "Start at login", true, view.autostart, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset_position", "Reset position", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &hide,
            &refresh_item,
            &settings_item,
            &on_top,
            &autostart,
            &reset,
            &separator,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;

    app.manage(TrayChecks {
        always_on_top: on_top.clone(),
        autostart: autostart.clone(),
    });

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(TOOLTIP_INITIAL)
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "hide" => hide_main_window(app),
            "refresh" => refresh(),
            "settings" => open_settings(app),
            // The check item has already flipped itself; apply its new state.
            // The controls re-sync both items to what was really accepted.
            "always_on_top" => {
                let want = on_top.is_checked().unwrap_or(false);
                controls::set_always_on_top(app, want);
            }
            "autostart" => {
                let want = autostart.is_checked().unwrap_or(false);
                controls::set_autostart(app, want);
            }
            "reset_position" => {
                controls::reset_position(app);
                // It is the recovery path: make sure the widget is visible.
                show_main_window(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // On macOS a left click opens the menu instead.
            if cfg!(target_os = "macos") {
                return;
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, FixedOffset, Utc};
    use usage_core::rate::Projection;
    use usage_core::usage::Window;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 22, h, m, 0).unwrap()
    }

    fn state(status: Status, five: Option<(f64, Option<DateTime<Utc>>)>) -> WidgetState {
        WidgetState {
            status,
            five_hour: five.map(|(utilization, resets_at)| Window { utilization, resets_at }),
            seven_day: None,
            rate_pct_per_hr: None,
            projection: Projection::Unknown,
            fetched_at: at(4, 0),
        }
    }

    #[test]
    fn live_shows_percent_and_local_reset() {
        let s = state(Status::Live, Some((58.0, Some(at(19, 55)))));
        assert_eq!(tooltip_text(&s, &Utc), "5h 58% · resets 19:55");
    }

    #[test]
    fn rounds_to_nearest_integer() {
        let s = state(Status::Live, Some((104.5, Some(at(19, 55)))));
        assert_eq!(tooltip_text(&s, &Utc), "5h 105% · resets 19:55");
        let s = state(Status::Live, Some((57.4, None)));
        assert_eq!(tooltip_text(&s, &Utc), "5h 57%");
    }

    #[test]
    fn converts_reset_to_the_given_timezone() {
        let s = state(Status::Live, Some((58.0, Some(at(19, 55)))));
        let tz = FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap();
        assert_eq!(tooltip_text(&s, &tz), "5h 58% · resets 01:25");
        let tz = FixedOffset::west_opt(8 * 3600).unwrap();
        assert_eq!(tooltip_text(&s, &tz), "5h 58% · resets 11:55");
    }

    #[test]
    fn missing_reset_omits_it() {
        let s = state(Status::Live, Some((58.0, None)));
        assert_eq!(tooltip_text(&s, &Utc), "5h 58%");
    }

    #[test]
    fn stale_appends_marker() {
        let s = state(Status::Stale { since: at(18, 0) }, Some((58.0, Some(at(19, 55)))));
        assert_eq!(tooltip_text(&s, &Utc), "5h 58% · resets 19:55 (stale)");
        let s = state(Status::Stale { since: at(18, 0) }, Some((58.0, None)));
        assert_eq!(tooltip_text(&s, &Utc), "5h 58% (stale)");
    }

    #[test]
    fn no_five_hour_window_is_no_data() {
        let s = state(Status::Live, None);
        assert_eq!(tooltip_text(&s, &Utc), "Claude usage: no data");
        let s = state(Status::Stale { since: at(18, 0) }, None);
        assert_eq!(tooltip_text(&s, &Utc), "Claude usage: no data");
    }

    #[test]
    fn signed_out_prompts_sign_in() {
        let s = state(Status::SignedOut, None);
        assert_eq!(tooltip_text(&s, &Utc), "Claude usage: open Claude Code to sign in");
    }

    #[test]
    fn title_is_rounded_percent_or_empty() {
        assert_eq!(title_text(&state(Status::Live, Some((57.5, None)))), "58%");
        assert_eq!(title_text(&state(Status::Live, None)), "");
    }
}
