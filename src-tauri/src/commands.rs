use crate::codex_client::UsageSnapshot;
use crate::usage_service::{MonitorState, UsageState};
use serde::Serialize;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

async fn account_key(state: &RwLock<UsageState>, requested: Option<String>) -> String {
    if let Some(key) = requested {
        return key;
    }
    state
        .read()
        .await
        .active_account
        .as_ref()
        .map(|a| a.account_key.clone())
        .unwrap_or_else(|| "__signed_out__".into())
}

#[tauri::command]
pub async fn get_accounts(
    db: tauri::State<'_, Arc<crate::db::Db>>,
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
) -> Result<crate::accounts::AccountContext, String> {
    Ok(crate::accounts::AccountContext {
        active_account: state.read().await.active_account.clone(),
        accounts: db.list_accounts()?,
    })
}

#[tauri::command]
pub fn get_pricing() -> crate::pricing::Catalog {
    crate::pricing::catalog()
}

#[tauri::command]
pub async fn refresh_pricing(
    db: tauri::State<'_, Arc<crate::db::Db>>,
    app: tauri::AppHandle,
) -> Result<crate::pricing::Catalog, String> {
    let result = crate::pricing::refresh(&db, true).await;
    let _ = app.emit("pricing-updated", crate::pricing::catalog());
    result
}

#[tauri::command]
pub async fn get_limit_history(
    db: tauri::State<'_, Arc<crate::db::Db>>,
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    account_key: Option<String>,
    limit_id: Option<String>,
    window_kind: Option<String>,
    offset: Option<i64>,
) -> Result<crate::limit_history::LimitHistoryPage, String> {
    let key = self::account_key(&state, account_key).await;
    let db = db.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        db.get_limit_history_for(
            &key,
            limit_id.as_deref(),
            window_kind.as_deref(),
            offset.unwrap_or(0),
            chrono::Utc::now().timestamp(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_account_usage_days(
    db: tauri::State<'_, Arc<crate::db::Db>>,
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    account_key: Option<String>,
    offset: Option<i64>,
) -> Result<crate::limit_history::AccountDayPage, String> {
    let key = self::account_key(&state, account_key).await;
    let db = db.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        db.get_account_usage_days_for(&key, offset.unwrap_or(0))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Tauri command: Get current usage snapshot
#[tauri::command]
pub async fn get_usage(
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    db: tauri::State<'_, Arc<crate::db::Db>>,
    account_key: Option<String>,
) -> Result<Option<UsageSnapshot>, String> {
    let key = self::account_key(&state, account_key).await;
    let s = state.read().await;
    if s.active_account
        .as_ref()
        .is_some_and(|a| a.account_key == key)
    {
        Ok(s.snapshot.clone())
    } else {
        db.cached_snapshot(&key)
    }
}

/// Tauri command: Get authoritative account-level token activity
#[tauri::command]
pub async fn get_account_usage(
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    db: tauri::State<'_, Arc<crate::db::Db>>,
    account_key: Option<String>,
) -> Result<Option<crate::codex_client::AccountUsage>, String> {
    let key = self::account_key(&state, account_key).await;
    let s = state.read().await;
    if s.active_account
        .as_ref()
        .is_some_and(|a| a.account_key == key)
    {
        Ok(s.account_usage.clone())
    } else {
        let mut usage = db.cached_account_usage(&key)?;
        if let Some(u) = &mut usage {
            u.account_key = Some(key);
        }
        Ok(usage)
    }
}

/// Tauri command: Get current token totals
#[tauri::command]
pub async fn get_token_totals(
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    db: tauri::State<'_, Arc<crate::db::Db>>,
    account_key: Option<String>,
) -> Result<crate::db::TokenTotals, String> {
    let key = self::account_key(&state, account_key).await;
    let s = state.read().await;
    let snapshot = db.cached_snapshot(&key)?;
    let now = chrono::Utc::now().timestamp();
    let (start5, end5) =
        crate::usage_service::token_window_bounds(snapshot.as_ref(), "fiveHour", 5 * 3600, now);
    let (start7, end7) =
        crate::usage_service::token_window_bounds(snapshot.as_ref(), "weekly", 7 * 24 * 3600, now);
    let session_start = if s
        .active_account
        .as_ref()
        .is_some_and(|a| a.account_key == key)
    {
        s.session_start_ts
    } else {
        now
    };
    db.get_token_totals(&key, session_start, start5, end5, start7, end7)
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
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    account_key: Option<String>,
    window_kind: String,
    since_hours: Option<u64>,
) -> Result<Vec<crate::db::QuotaSampleRow>, String> {
    let key = self::account_key(&state, account_key).await;
    let hours = since_hours.unwrap_or(168); // default: 1 week
    let since_ts = chrono::Utc::now().timestamp() - (hours as i64 * 3600);
    db.get_recent_quota_samples(&key, &window_kind, since_ts, 500)
}

/// Tauri command: Get recent app events for the event log
#[tauri::command]
pub async fn get_recent_events(
    db: tauri::State<'_, Arc<crate::db::Db>>,
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    account_key: Option<String>,
) -> Result<Vec<crate::db::AppEvent>, String> {
    let key = self::account_key(&state, account_key).await;
    db.get_recent_events(&key, 20)
}

/// Tauri command: Get usage deltas derived from quota samples
#[tauri::command]
pub async fn get_usage_deltas(
    db: tauri::State<'_, Arc<crate::db::Db>>,
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    account_key: Option<String>,
) -> Result<crate::db::UsageDeltas, String> {
    let key = self::account_key(&state, account_key).await;
    let s = state.read().await;
    db.get_usage_deltas(
        &key,
        "weekly",
        if s.active_account
            .as_ref()
            .is_some_and(|a| a.account_key == key)
        {
            s.session_start_ts
        } else {
            chrono::Utc::now().timestamp()
        },
    )
}

/// Tauri command: List locally stored Codex chats with token and cost summaries.
#[tauri::command]
pub async fn get_chat_sessions(
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    account_key: Option<String>,
) -> Result<Vec<crate::session_history::ChatSessionSummary>, String> {
    let key = self::account_key(&state, account_key).await;
    tauri::async_runtime::spawn_blocking(move || {
        crate::session_history::list_chat_sessions_for(&key)
    })
    .await
    .map_err(|error| format!("Session history task failed: {error}"))?
}

/// Tauri command: Read messages and per-request usage for one locally stored chat.
#[tauri::command]
pub async fn get_chat_session_detail(
    session_id: String,
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    account_key: Option<String>,
) -> Result<crate::session_history::ChatSessionDetail, String> {
    let key = self::account_key(&state, account_key).await;
    tauri::async_runtime::spawn_blocking(move || {
        let detail = crate::session_history::read_chat_session(&session_id)?;
        if detail.summary.account_key != key {
            return Err("Chat belongs to another account".into());
        }
        Ok(detail)
    })
    .await
    .map_err(|error| format!("Session detail task failed: {error}"))?
}

/// Tauri command: Read the most recently active Codex chat.
#[tauri::command]
pub async fn get_current_chat_summary(
    state: tauri::State<'_, Arc<RwLock<UsageState>>>,
    account_key: Option<String>,
) -> Result<Option<crate::session_history::ChatSessionSummary>, String> {
    let key = self::account_key(&state, account_key).await;
    tauri::async_runtime::spawn_blocking(move || {
        crate::session_history::list_chat_sessions_for(&key).map(|rows| rows.into_iter().next())
    })
    .await
    .map_err(|error| format!("Current chat task failed: {error}"))?
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
