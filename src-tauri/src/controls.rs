//! Window and settings actions shared by the front-end commands and the tray
//! menu. Every action returns (and broadcasts) the state the backend actually
//! ended up in, so neither the panel nor the tray can show something that was
//! not accepted.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Position, Wry};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_window_state::{AppHandleExt as _, StateFlags};

use crate::placement::{self, Rect};
use crate::settings::{self, Settings};

pub const MAIN_WINDOW: &str = "main";
/// Emitted with a [`SettingsView`] whenever a setting changes.
pub const SETTINGS_CHANGED_EVENT: &str = "settings-changed";
/// Emitted (no payload) to ask the panel to open the settings view.
pub const OPEN_SETTINGS_EVENT: &str = "open-settings";

/// Settings in memory plus the file they are saved to (none if the config dir
/// could not be determined: then they simply do not persist).
pub struct SettingsState {
    path: Option<PathBuf>,
    current: Mutex<Settings>,
}

impl SettingsState {
    pub fn load(path: Option<PathBuf>) -> Self {
        let current = path.as_deref().map(settings::load).unwrap_or_default();
        Self { path, current: Mutex::new(current) }
    }

    fn get(&self) -> Settings {
        self.current.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Change the in-memory value only (nothing is written to disk).
    fn set_in_memory(&self, change: impl FnOnce(&mut Settings)) {
        change(&mut self.current.lock().unwrap_or_else(|e| e.into_inner()));
    }

    fn update(&self, change: impl FnOnce(&mut Settings)) {
        let mut guard = self.current.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut guard);
        if let Some(path) = &self.path {
            if let Err(err) = settings::save(path, &guard) {
                eprintln!("could not save settings: {err}");
            }
        }
    }
}

/// The tray's check items, kept so they can be synced from either side.
pub struct TrayChecks {
    pub always_on_top: CheckMenuItem<Wry>,
    pub autostart: CheckMenuItem<Wry>,
}

/// Everything the settings panel shows. Contains no credentials.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingsView {
    pub always_on_top: bool,
    pub autostart: bool,
    pub version: String,
    /// Configured (tauri.conf.json) logical size of the widget: the usage view.
    pub window_width: f64,
    pub window_height: f64,
}

/// Fallback if the config has no main window entry.
const FALLBACK_SIZE: (f64, f64) = (260.0, 150.0);
/// Upper bound for a height requested by the front end.
const MAX_HEIGHT: f64 = 600.0;

fn configured_size(app: &AppHandle) -> (f64, f64) {
    app.config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN_WINDOW)
        .map(|w| (w.width, w.height))
        .unwrap_or(FALLBACK_SIZE)
}

/// Resize to a logical size, keeping the top-left corner where it was (some
/// platforms keep the bottom-left fixed on resize).
fn resize_keep_corner(app: &AppHandle, width: f64, height: f64) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let before = window.outer_position().ok();
    if window.set_size(LogicalSize::new(width, height)).is_ok() {
        if let Some(pos) = before {
            let _ = window.set_position(pos);
        }
    }
}

/// Back to the configured size; used on every page load so a webview reload
/// never leaves the window at the settings height.
pub fn reset_size(app: &AppHandle) {
    let (w, h) = configured_size(app);
    resize_keep_corner(app, w, h);
}

fn view(app: &AppHandle) -> SettingsView {
    let (window_width, window_height) = configured_size(app);
    SettingsView {
        always_on_top: app.state::<SettingsState>().get().always_on_top,
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        version: app.package_info().version.to_string(),
        window_width,
        window_height,
    }
}

/// Mirror `view` into the tray check items and tell the panel.
fn publish(app: &AppHandle, view: &SettingsView) {
    if let Some(checks) = app.try_state::<TrayChecks>() {
        let _ = checks.always_on_top.set_checked(view.always_on_top);
        let _ = checks.autostart.set_checked(view.autostart);
    }
    let _ = app.emit(SETTINGS_CHANGED_EVENT, view);
}

pub fn current(app: &AppHandle) -> SettingsView {
    view(app)
}

/// Apply the stored always-on-top value to the window. Called once at startup.
pub fn apply_stored(app: &AppHandle) {
    let state = app.state::<SettingsState>();
    let want = state.get().always_on_top;
    let applied = app
        .get_webview_window(MAIN_WINDOW)
        .map(|w| w.set_always_on_top(want).is_ok())
        .unwrap_or(false);
    // If it could not be applied, do not claim it is on (and do not save).
    if want && !applied {
        state.set_in_memory(|s| s.always_on_top = false);
    }
}

pub fn set_always_on_top(app: &AppHandle, value: bool) -> SettingsView {
    // Only persist what the window accepted.
    let applied = app
        .get_webview_window(MAIN_WINDOW)
        .map(|w| w.set_always_on_top(value).is_ok())
        .unwrap_or(false);
    if applied {
        app.state::<SettingsState>().update(|s| s.always_on_top = value);
    }
    let view = view(app);
    publish(app, &view);
    view
}

pub fn set_autostart(app: &AppHandle, value: bool) -> SettingsView {
    let manager = app.autolaunch();
    let _ = if value { manager.enable() } else { manager.disable() };
    // `view` re-reads the real OS state, so a failure shows as unchanged.
    let view = view(app);
    publish(app, &view);
    view
}

fn rect_of(area: &tauri::PhysicalRect<i32, u32>) -> Rect {
    Rect::new(area.position.x, area.position.y, area.size.width, area.size.height)
}

/// Move the widget near the top-right of the primary monitor's work area and
/// save the position. All arithmetic uses the target monitor's scale factor.
pub fn reset_position(app: &AppHandle) -> SettingsView {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let monitor = window
            .primary_monitor()
            .ok()
            .flatten()
            .or_else(|| window.available_monitors().ok().and_then(|m| m.into_iter().next()));
        if let (Some(monitor), Ok(size), Ok(window_scale)) =
            (monitor, window.outer_size(), window.scale_factor())
        {
            let target = placement::reset_target(
                rect_of(monitor.work_area()),
                monitor.scale_factor(),
                size.width,
                window_scale,
                placement::DEFAULT_MARGIN,
            );
            // macOS converts a physical position with the window's current scale,
            // not the target monitor's, so give it logical units there.
            let position: Position = if cfg!(target_os = "macos") {
                tauri::LogicalPosition::new(target.logical.0, target.logical.1).into()
            } else {
                PhysicalPosition::new(target.physical.0, target.physical.1).into()
            };
            if window.set_position(position).is_ok() {
                let _ = app.save_window_state(StateFlags::POSITION);
            }
        }
    }
    view(app)
}

/// Startup guard: if the restored position leaves the grip strip unreachable
/// on every monitor, fall back to the default position.
pub fn ensure_on_screen(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let (Ok(pos), Ok(size), Ok(scale), Ok(monitors)) = (
        window.outer_position(),
        window.outer_size(),
        window.scale_factor(),
        window.available_monitors(),
    ) else {
        return;
    };
    let areas: Vec<Rect> = monitors.iter().map(|m| rect_of(m.work_area())).collect();
    if areas.is_empty() {
        return; // cannot judge without any monitor information
    }
    let grip_h = (placement::GRIP_HEIGHT * scale).round() as u32;
    let min_w = (placement::GRIP_MIN_VISIBLE_WIDTH * scale).round() as u32;
    let rect = Rect::new(pos.x, pos.y, size.width, size.height);
    if !placement::grip_reachable(rect, &areas, grip_h, min_w) {
        reset_position(app);
    }
}

#[tauri::command]
pub fn set_widget_height_cmd(app: AppHandle, height: f64) {
    let (width, base) = configured_size(&app);
    if height.is_finite() && (base..=MAX_HEIGHT).contains(&height) {
        resize_keep_corner(&app, width, height);
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> SettingsView {
    current(&app)
}

#[tauri::command]
pub fn set_always_on_top_cmd(app: AppHandle, value: bool) -> SettingsView {
    set_always_on_top(&app, value)
}

#[tauri::command]
pub fn set_autostart_cmd(app: AppHandle, value: bool) -> SettingsView {
    set_autostart(&app, value)
}

#[tauri::command]
pub fn reset_position_cmd(app: AppHandle) -> SettingsView {
    let view = reset_position(&app);
    publish(&app, &view);
    view
}
