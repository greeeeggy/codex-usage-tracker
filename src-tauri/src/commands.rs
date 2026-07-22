use crate::codex_client::UsageSnapshot;
use crate::usage_service::{MonitorState, UsageState};
use serde::Serialize;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

/// Tauri command: Get current usage snapshot
#[tauri::command]
pub async fn get_usage(
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
) -> Result<Option<UsageSnapshot>, String> {
    let s = state.read().await;
    Ok(s.snapshot.clone())
}

/// Tauri command: Get current token totals
#[tauri::command]
pub async fn get_token_totals(
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
) -> Result<crate::db::TokenTotals, String> {
    let s = state.read().await;
    Ok(s.token_totals.clone())
}

/// Tauri command: Get current monitor state
#[tauri::command]
pub async fn get_monitor_state(
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
) -> Result<MonitorStateResponse, String> {
    let s = state.read().await;
    Ok(MonitorStateResponse {
        state: s.monitor_state.clone(),
        error_message: s.error_message.clone(),
        detected_clients: s
            .detected_clients
            .iter()
            .map(|c| DetectedClientInfo {
                client_type: format!("{:?}", c.client_type),
                name: c.name.clone(),
            })
            .collect(),
    })
}

/// Tauri command: Force refresh usage data
#[tauri::command]
pub async fn refresh_usage(app: tauri::AppHandle) -> Result<(), String> {
    // Emit a refresh-requested event that the usage service listens to
    app.emit("refresh-requested", ())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Tauri command: Get quota sample history for charts
#[tauri::command]
pub async fn get_quota_history(
    db: tauri::State<'_, Arc<crate::db::Db>>,
    window_kind: String,
    since_hours: Option<u64>,
) -> Result<Vec<crate::db::QuotaSampleRow>, String> {
    let hours = since_hours.unwrap_or(168); // default: 1 week
    let since_ts = chrono::Utc::now().timestamp() - (hours as i64 * 3600);
    db.get_recent_quota_samples(&window_kind, since_ts, 500)
}

/// Tauri command: Get recent app events for the event log
#[tauri::command]
pub async fn get_recent_events(
    db: tauri::State<'_, Arc<crate::db::Db>>,
) -> Result<Vec<crate::db::AppEvent>, String> {
    db.get_recent_events(20)
}

/// Tauri command: Get usage deltas derived from quota samples
#[tauri::command]
pub async fn get_usage_deltas(
    db: tauri::State<'_, Arc<crate::db::Db>>,
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
) -> Result<crate::db::UsageDeltas, String> {
    let s = state.read().await;
    db.get_usage_deltas("weekly", s.session_start_ts)
}

/// Tauri command: Toggle the overlay widget visibility
#[tauri::command]
pub async fn toggle_widget(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("overlay") {
        if window.is_visible().unwrap_or(false) {
            window.hide().map_err(|e| e.to_string())?;
        } else {
            window.show().map_err(|e| e.to_string())?;
            window.set_focus().map_err(|e| e.to_string())?;
            window.unminimize().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Tauri command: Show the main dashboard window
#[tauri::command]
pub async fn show_dashboard(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        window.unminimize().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Response for get_monitor_state
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorStateResponse {
    pub state: MonitorState,
    pub error_message: Option<String>,
    pub detected_clients: Vec<DetectedClientInfo>,
}

/// Info about a detected Codex client
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedClientInfo {
    pub client_type: String,
    pub name: String,
}

