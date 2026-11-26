// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod poller;
mod tray;

use std::sync::{Arc, Mutex};

use chrono::Utc;
use poller::{Poller, WidgetState, PRODUCTION_BASE};
use tauri::{Emitter, Manager, State};
use tokio::sync::Notify;

/// Latest state, so a webview that loads after the first event can fetch it.
#[derive(Default)]
struct LatestState(Mutex<Option<WidgetState>>);

struct Refresh(Arc<Notify>);

#[tauri::command]
fn refresh_now(refresh: State<'_, Refresh>) {
    refresh.0.notify_one();
}

#[tauri::command]
fn get_state(latest: State<'_, LatestState>) -> Option<WidgetState> {
    latest.0.lock().ok().and_then(|g| g.clone())
}

fn main() {
    tauri::Builder::default()
        // A second launch would double the polling: just surface the first.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        // Remember where the widget was dragged to; position only.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::POSITION)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(LatestState::default())
        .manage(Refresh(Arc::new(Notify::new())))
        .invoke_handler(tauri::generate_handler![refresh_now, get_state])
        // Closing the window hides it; the app exits only via the tray Quit item.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let samples_path = app.path().app_data_dir().ok().and_then(|dir| {
                std::fs::create_dir_all(&dir).ok()?;
                Some(dir.join("samples.jsonl"))
            });
            let poller = Poller::new(
                PRODUCTION_BASE,
                Box::new(usage_core::credentials::read_access_token),
                samples_path,
                Utc::now(),
            );
            let refresh = app.state::<Refresh>().0.clone();
            let tray_refresh = refresh.clone();
            tray::build(app.handle(), move || tray_refresh.notify_one())?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(poller::run(poller, refresh, move |state| {
                if let Ok(mut latest) = handle.state::<LatestState>().0.lock() {
                    *latest = Some(state.clone());
                }
                tray::update(&handle, &state);
                let _ = handle.emit("usage-update", &state);
            }));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Claude Usage");
}
