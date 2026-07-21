mod codex_client;
mod commands;
mod db;
mod local_server;
mod process_detector;
mod usage_service;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, Listener,
    WindowEvent,
};

use usage_service::UsageService;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Create a dedicated Tokio runtime for all async work.
            // Tauri 2 does not provide an ambient Tokio runtime in setup,
            // so we must create our own and keep it alive for the process lifetime.
            let rt = tokio::runtime::Runtime::new()
                .expect("failed to create Tokio runtime");
            // Leak a 'static reference so spawned tasks live for the process lifetime
            let rt: &'static tokio::runtime::Runtime = Box::leak(Box::new(rt));

            // Create the shared usage state
            let usage_service = UsageService::new(app_handle.clone());
            let shared_state = usage_service.state();

            // Store the shared state for Tauri commands
            app.manage(shared_state.clone());

            // Create SSE broadcast channel for local server
            let (sse_tx, _) = tokio::sync::broadcast::channel::<String>(100);
            let sse_tx_clone = sse_tx.clone();

            // Subscribe to usage-updated events to broadcast via SSE
            let sse_tx_for_events = sse_tx.clone();
            let state_for_sse = shared_state.clone();
            let handle = rt.handle().clone();
            app_handle.listen("usage-updated", move |_event| {
                let state = state_for_sse.clone();
                let tx = sse_tx_for_events.clone();
                handle.spawn(async move {
                    let s = state.read().await;
                    if let Some(snapshot) = &s.snapshot {
                        // Build the simplified JSON for SSE/API consumers
                        let mut data = serde_json::Map::new();
                        for window in &snapshot.windows {
                            data.insert(
                                window.name.clone(),
                                serde_json::json!({
                                    "remainingPercent": window.remaining_percent,
                                    "usedPercent": window.used_percent,
                                    "resetsAt": window.resets_at,
                                    "durationMinutes": window.duration_minutes,
                                }),
                            );
                        }
                        data.insert(
                            "planType".to_string(),
                            serde_json::json!(snapshot.plan_type),
                        );
                        data.insert(
                            "capturedAt".to_string(),
                            serde_json::json!(snapshot.captured_at),
                        );
                        let json = serde_json::to_string(&data).unwrap_or_default();
                        let _ = tx.send(json);
                    }
                });
            });

            // Build tray menu
            let show_dashboard =
                MenuItem::with_id(app, "dashboard", "Open Dashboard", true, None::<&str>)?;
            let toggle_widget =
                MenuItem::with_id(app, "toggle_widget", "Show/Hide Widget", true, None::<&str>)?;
            let refresh = MenuItem::with_id(app, "refresh", "Refresh Now", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&show_dashboard, &toggle_widget, &refresh, &quit])?;

            // Create tray icon
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Codex Meter — Initializing...")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "dashboard" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.unminimize();
                        }
                    }
                    "toggle_widget" => {
                        if let Some(window) = app.get_webview_window("overlay") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = window.unminimize();
                            }
                        }
                    }
                    "refresh" => {
                        let _ = app.emit("refresh-requested", ());
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(move |tray, event| {
                    match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            // Left click toggles the widget
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("overlay") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                    let _ = window.unminimize();
                                }
                            }
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            // Start the local HTTP server
            let state_for_server = shared_state.clone();
            rt.spawn(async move {
                local_server::start_server(state_for_server, sse_tx_clone).await;
            });

            // Start the usage monitoring service
            rt.spawn(async move {
                usage_service.run().await;
            });

            // Update tray tooltip periodically
            let state_for_tooltip = shared_state.clone();
            rt.spawn(async move {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    let s = state_for_tooltip.read().await;
                    if let Some(snapshot) = &s.snapshot {
                        let parts: Vec<String> = snapshot
                            .windows
                            .iter()
                            .map(|w| {
                                let label = match w.name.as_str() {
                                    "fiveHour" => "5h",
                                    "weekly" => "Wk",
                                    _ => &w.name,
                                };
                                format!("{}: {:.0}%", label, w.remaining_percent)
                            })
                            .collect();
                        let tooltip = format!("Codex: {}", parts.join(" · "));
                        // Tray tooltip update would go here
                        // (Tauri 2 tray tooltip updates require the tray handle)
                        log::debug!("Tray: {}", tooltip);
                    }
                }
            });

            // Hide main window on startup (tray-only)
            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.hide();

                // Intercept the close button — hide instead of exiting
                let main_window_clone = main_window.clone();
                main_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        // Hide the window so the app keeps running in tray
                        let _ = main_window_clone.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_usage,
            commands::get_token_totals,
            commands::get_monitor_state,
            commands::refresh_usage,
            commands::toggle_widget,
            commands::show_dashboard,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
