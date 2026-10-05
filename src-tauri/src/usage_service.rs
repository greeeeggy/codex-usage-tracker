use crate::codex_client::{CodexClient, CodexEvent, UsageSnapshot};
use crate::process_detector;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Emitter, Listener};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{Duration, Instant};

/// Monitor state machine states
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MonitorState {
    #[default]
    Dormant,
    Connecting,
    Monitoring,
    GracePeriod,
    AuthRequired,
    Error,
}

/// Alert state for notification thresholds — resets each time Codex is reopened
#[derive(Debug, Clone, Default)]
struct AlertState {
    window_name: String,
    alerted_75: bool,
    alerted_50: bool,
    alerted_25: bool,
    alerted_0: bool,
}

/// Shared usage state accessible from Tauri commands
#[derive(Debug, Clone, Default)]
pub struct UsageState {
    pub active_account: Option<crate::accounts::AccountProfile>,
    pub snapshot: Option<UsageSnapshot>,
    pub account_usage: Option<crate::codex_client::AccountUsage>,
    pub monitor_state: MonitorState,
    pub error_message: Option<String>,
    pub detected_clients: Vec<process_detector::DetectedClient>,
    pub token_totals: crate::db::TokenTotals,
    pub usage_deltas: crate::db::UsageDeltas,
    /// Timestamp when the current monitoring session started (for delta calculations)
    pub session_start_ts: i64,
}

/// The main usage service that manages the monitoring lifecycle
pub struct UsageService {
    state: Arc<RwLock<UsageState>>,
    app_handle: tauri::AppHandle,
    db: Arc<crate::db::Db>,
}

impl UsageService {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        let db = Arc::new(crate::db::Db::new(&app_handle).expect("Failed to init DB"));
        let now = chrono::Utc::now().timestamp();
        let initial_state = UsageState {
            session_start_ts: now,
            ..UsageState::default()
        };

        Self {
            state: Arc::new(RwLock::new(initial_state)),
            app_handle,
            db,
        }
    }

    /// Get a reference to the shared state
    pub fn state(&self) -> Arc<RwLock<UsageState>> {
        self.state.clone()
    }

    /// Get a reference to the database
    pub fn db(&self) -> Arc<crate::db::Db> {
        self.db.clone()
    }

    /// Start the monitoring loop
    pub async fn run(self) {
        let state = self.state.clone();
        let app_handle = self.app_handle.clone();
        let db_clone = self.db.clone();
        let db_for_samples = self.db.clone();

        let mut next_connect_at = Instant::now();
        let mut client: Option<CodexClient> = None;
        let mut refresh_interval = tokio::time::interval(Duration::from_secs(30));
        let mut process_check_interval = tokio::time::interval(Duration::from_secs(3));
        let mut account_usage_interval = tokio::time::interval_at(
            Instant::now() + Duration::from_secs(300),
            Duration::from_secs(300),
        );
        let mut alert_states: Vec<AlertState> = Vec::new();

        // Channel for receiving events from the Codex client
        let (mut event_tx, mut event_rx) = mpsc::unbounded_channel::<CodexEvent>();
        let mut reconnect = false;
        let mut local_identity = crate::accounts::local_account()
            .ok()
            .flatten()
            .map(|a| a.account_key);

        // Internal channel for manual refresh
        let (internal_tx, mut internal_rx) = mpsc::unbounded_channel::<()>();
        app_handle.listen("refresh-requested", move |_| {
            let _ = internal_tx.send(());
        });

        // Start the session log watcher in the background
        let db_for_watcher = self.db.clone();
        let state_for_watcher = self.state.clone();
        let app_handle_for_watcher = self.app_handle.clone();
        tokio::spawn(async move {
            crate::session_log_watcher::run(
                db_for_watcher,
                app_handle_for_watcher,
                state_for_watcher,
            )
            .await;
        });

        loop {
            if reconnect {
                if let Some(old_client) = client.take() {
                    old_client.stop();
                }
                // Drop queued events from the old connection, including its late disconnect.
                (event_tx, event_rx) = mpsc::unbounded_channel();
                reset_account_state(&mut *state.write().await);
                db_clone.set_active_account("__signed_out__");
                alert_states.clear();
                emit_accounts(&db_clone, &state, &app_handle).await;
                let _ = app_handle.emit("state-changed", "dormant");
                next_connect_at = Instant::now();
                reconnect = false;
            }
            tokio::select! {
                // Process detection tick
                _ = process_check_interval.tick() => {
                    if let Ok(account) = crate::accounts::local_account() {
                        let key = account.map(|a| a.account_key);
                        if key != local_identity {
                            local_identity = key;
                            reconnect = true;
                            continue;
                        }
                    }
                    let clients = process_detector::detect_codex_clients();
                    let has_clients = !clients.is_empty();
                    if has_clients {
                        log::info!("[DETECT] Found {} Codex client(s): {:?}", clients.len(), clients);
                    }

                    let current_state = {
                        let mut s = state.write().await;
                        s.detected_clients = clients;
                        s.monitor_state.clone()
                    };

                    match (&current_state, has_clients) {
                        (MonitorState::Dormant, _) if Instant::now() >= next_connect_at => {
                            // The shared allowance can be used on the web or in
                            // cloud Work while no local client is open.
                            next_connect_at = Instant::now() + Duration::from_secs(30);
                            log::info!("Starting account-wide quota monitoring...");
                            let session_ts = chrono::Utc::now().timestamp();
                            {
                                let mut s = state.write().await;
                                s.monitor_state = MonitorState::Connecting;
                                s.error_message = None;
                                s.session_start_ts = session_ts;
                            }
                            let _ = app_handle.emit("state-changed", "connecting");

                            // Start Codex client
                            let mut new_client = CodexClient::new(event_tx.clone());
                            log::info!("[CONNECT] Attempting to start Codex client...");
                            match new_client.start().await {
                                Ok(_) => {
                                    log::info!("[CONNECT] Connected to Codex app-server successfully");
                                    let Some(account) = new_client.account() else {
                                        new_client.stop();
                                        let mut s = state.write().await;
                                        s.monitor_state = MonitorState::AuthRequired;
                                        s.error_message = Some("Sign in to a ChatGPT account in Codex to monitor its usage.".into());
                                        let _ = app_handle.emit("state-changed", "authRequired");
                                        continue;
                                    };
                                    let account_key = account.account_key.clone();
                                    if let Err(e) = db_clone.remember_account(&account) { log::warn!("Failed to save account profile: {}", e); }
                                    db_clone.set_active_account(&account_key);
                                    let _ = db_clone.insert_app_event("session_started", "Monitoring started", Some("Monitoring the signed-in account"));
                                    {
                                        let mut s = state.write().await;
                                        s.active_account = Some(account);
                                        s.snapshot = db_clone.cached_snapshot(&account_key).ok().flatten();
                                        s.account_usage = db_clone.cached_account_usage(&account_key).ok().flatten();
                                    }
                                    emit_accounts(&db_clone, &state, &app_handle).await;

                                    let _ = db_for_samples.insert_app_event(
                                        "monitoring_resumed",
                                        "Connected to Codex",
                                        Some("App-server connection established"),
                                    );

                                    // Fetch initial usage
                                    log::info!("[DATA] Fetching initial rate limits...");
                                    match new_client.read_rate_limits().await {
                                        Ok(snapshot) => {
                                            log::info!("[DATA] Got snapshot: plan={:?}, windows={}", snapshot.plan_type, snapshot.windows.len());
                                            for w in &snapshot.windows {
                                                log::info!("[DATA]   window '{}': used={:.1}% remaining={:.1}% resets_at={:?}", w.name, w.used_percent, w.remaining_percent, w.resets_at);
                                            }

                                            // Persist quota samples
                                            persist_quota_samples(&db_for_samples, &snapshot);

                                            // Fire an on-open status notification
                                            let status_parts: Vec<String> = snapshot.windows.iter().map(|w| {
                                                let label = match w.name.as_str() {
                                                    "fiveHour" => "5h",
                                                    "weekly" => "weekly",
                                                    _ => &w.name,
                                                };
                                                format!("{}: {:.0}% remaining", label, w.remaining_percent)
                                            }).collect();
                                            send_notification(
                                                &app_handle,
                                                "Usage monitor connected",
                                                &status_parts.join(" · "),
                                            );

                                            let mut s = state.write().await;
                                            let snapshot = update_snapshot_state(&mut s, snapshot);
                                            s.monitor_state = MonitorState::Monitoring;

                                            // Update usage deltas
                                            if let Ok(deltas) = db_for_samples.get_usage_deltas(&account_key, "weekly", session_ts) {
                                                s.usage_deltas = deltas;
                                            }

                                            let _ = app_handle.emit("usage-updated", &snapshot);
                                            let _ = app_handle.emit("state-changed", "monitoring");

                                            let _ = db_for_samples.insert_app_event(
                                                "usage_updated",
                                                "Initial data received",
                                                Some(&format!("Plan: {:?}", snapshot.plan_type)),
                                            );
                                        }
                                        Err(e) => {
                                            log::error!("[DATA] FAILED to read initial rate limits: {}", e);
                                            let mut s = state.write().await;
                                            s.monitor_state = MonitorState::Error;
                                            s.error_message = Some(e);
                                            let _ = app_handle.emit("state-changed", &s.monitor_state);
                                        }
                                    }

                                    if let Err(e) = refresh_token_totals(
                                        &db_clone,
                                        &state,
                                        &app_handle,
                                    )
                                    .await
                                    {
                                        log::warn!("Failed to hydrate token totals: {}", e);
                                    }

                                    if let Err(e) = refresh_account_usage(
                                        &new_client,
                                        &db_clone,
                                        &state,
                                        &app_handle,
                                    )
                                    .await
                                    {
                                        log::warn!("Account token usage is unavailable: {}", e);
                                    }

                                    client = Some(new_client);
                                }
                                Err(e) => {
                                    new_client.stop();
                                    log::error!("[CONNECT] FAILED to start Codex client: {}", e);
                                    let _ = db_for_samples.insert_app_event(
                                        "refresh_failed",
                                        "Connection failed",
                                        Some(&e),
                                    );
                                    let mut s = state.write().await;
                                    if e.contains("not found") {
                                        s.monitor_state = MonitorState::Error;
                                        s.error_message = Some("Codex CLI not found. Install it with: npm install -g @openai/codex".to_string());
                                    } else if e.contains("auth") || e.contains("login") {
                                        s.monitor_state = MonitorState::AuthRequired;
                                        s.error_message = Some("Please sign in to Codex: run 'codex login' in your terminal".to_string());
                                    } else {
                                        s.monitor_state = MonitorState::Error;
                                        s.error_message = Some(e);
                                    }
                                    let _ = app_handle.emit("state-changed", &s.monitor_state);
                                }
                            }
                        }

                        (MonitorState::Error | MonitorState::AuthRequired, _) if client.is_none() && Instant::now() >= next_connect_at => {
                            // Retry connection after error
                            {
                                let mut s = state.write().await;
                                s.monitor_state = MonitorState::Dormant;
                                s.error_message = None;
                            }
                        }

                        _ => {}
                    }
                }

                // Periodic refresh tick (30 seconds)
                _ = refresh_interval.tick() => {
                    if client.is_some() {
                        if let Some(c) = &client {
                            let previous = c.account().map(|a| a.account_key);
                            match c.read_account().await {
                                Ok(account) if account.as_ref().map(|a| &a.account_key) != previous.as_ref() => { reconnect = true; continue; }
                                Err(e) => { log::warn!("Failed to verify signed-in account: {}", e); reconnect = true; continue; }
                                _ => {}
                            }
                            match c.read_rate_limits().await {
                                Ok(snapshot) => {
                                    // Persist quota samples
                                    persist_quota_samples(&db_for_samples, &snapshot);

                                    let session_ts = state.read().await.session_start_ts;
                                    let snapshot = {
                                        let mut s = state.write().await;
                                        let snapshot = update_snapshot_state(&mut s, snapshot);

                                        // Update usage deltas
                                        if let Ok(deltas) = db_for_samples.get_usage_deltas(snapshot.account_key.as_deref().unwrap_or("__signed_out__"), "weekly", session_ts) {
                                            s.usage_deltas = deltas;
                                        }
                                        snapshot
                                    };

                                    if let Err(e) = refresh_token_totals(&db_clone, &state, &app_handle).await {
                                        log::warn!("Failed to refresh token totals: {}", e);
                                    }

                                    let _ = app_handle.emit("usage-updated", &snapshot);
                                    let _ = app_handle.emit("state-changed", "monitoring");

                                    // Check notification thresholds
                                    check_notifications(&snapshot, &mut alert_states, &app_handle);
                                }
                                Err(e) => {
                                    if e.contains("account changed") { reconnect = true; continue; }
                                    log::warn!("Failed to refresh rate limits: {}", e);
                                    let mut s = state.write().await;
                                    s.monitor_state = MonitorState::Error;
                                    s.error_message = Some(e);
                                    let _ = app_handle.emit("state-changed", &s.monitor_state);
                                }
                            }
                        }
                    }
                }

                // Events from Codex client (push notifications)
                Some(event) = event_rx.recv() => {
                    match event {
                        CodexEvent::UsageUpdated(snapshot) => {
                            if client.as_ref().is_none_or(|c| !c.has_current_identity()) { reconnect = true; continue; }
                            if snapshot.account_key.as_deref() != state.read().await.active_account.as_ref().map(|a| a.account_key.as_str()) || snapshot.account_key.is_none() { continue; }
                            log::info!("Received push usage update");

                            // Persist quota samples
                            persist_quota_samples(&db_for_samples, &snapshot);

                            let _ = db_for_samples.insert_app_event(
                                "usage_updated",
                                "Rate limits updated",
                                Some(&format!("{} windows", snapshot.windows.len())),
                            );

                            let session_ts = state.read().await.session_start_ts;
                            let snapshot = {
                                let mut s = state.write().await;
                                let snapshot = update_snapshot_state(&mut s, snapshot);

                                // Update usage deltas
                                if let Ok(deltas) = db_for_samples.get_usage_deltas(snapshot.account_key.as_deref().unwrap_or("__signed_out__"), "weekly", session_ts) {
                                    s.usage_deltas = deltas;
                                }
                                snapshot
                            };

                            if let Err(e) = refresh_token_totals(&db_clone, &state, &app_handle).await {
                                log::warn!("Failed to refresh token totals: {}", e);
                            }

                            let _ = app_handle.emit("usage-updated", &snapshot);

                            check_notifications(&snapshot, &mut alert_states, &app_handle);
                        }
                        CodexEvent::TokenUsageUpdated(token_event) => {
                            if client.as_ref().is_none_or(|c| !c.has_current_identity()) { reconnect = true; continue; }
                            if Some(token_event.account_key.as_str()) != state.read().await.active_account.as_ref().map(|a| a.account_key.as_str()) { continue; }
                            log::info!("Received token usage update: {} tokens", token_event.total_tokens);
                            if let Err(e) = db_clone.insert_token_event(&token_event) {
                                log::error!("Failed to persist token event: {}", e);
                            } else if let Err(e) =
                                refresh_token_totals(&db_clone, &state, &app_handle).await
                            {
                                log::error!("Failed to calculate token totals: {}", e);
                            }
                        }
                        CodexEvent::AccountChanged => { reconnect = true; }
                        CodexEvent::Disconnected => {
                            if client.is_none() { continue; }
                            reconnect = true;
                            log::warn!("Codex app-server disconnected");
                            client = None;
                            next_connect_at = Instant::now() + Duration::from_secs(30);
                            let mut s = state.write().await;
                            if s.monitor_state != MonitorState::Dormant {
                                s.monitor_state = MonitorState::Dormant;
                                s.error_message = None;
                                alert_states.clear();
                                let _ = app_handle.emit("state-changed", "dormant");
                                let _ = db_for_samples.insert_app_event(
                                    "session_ended",
                                    "Codex disconnected",
                                    Some("App-server process terminated"),
                                );
                            }
                        }
                    }
                }

                // Account usage changes less frequently than the live rate-limit windows.
                _ = account_usage_interval.tick() => {
                    if client.is_some() {
                        if let Some(c) = &client {
                            if let Err(e) = refresh_account_usage(c, &db_clone, &state, &app_handle).await {
                                log::warn!("Failed to refresh account token usage: {}", e);
                            }
                        }
                    }
                }

                // Internal events
                Some(_) = internal_rx.recv() => {
                    log::info!("Manual refresh requested");
                    if client.is_none() { reconnect = true; continue; }
                    if client.is_some() {
                        if let Some(c) = &client {
                            match c.read_rate_limits().await {
                                Ok(snapshot) => {
                                    persist_quota_samples(&db_for_samples, &snapshot);

                                    let session_ts = state.read().await.session_start_ts;
                                    let snapshot = {
                                        let mut s = state.write().await;
                                        let snapshot = update_snapshot_state(&mut s, snapshot);

                                        if let Ok(deltas) = db_for_samples.get_usage_deltas(snapshot.account_key.as_deref().unwrap_or("__signed_out__"), "weekly", session_ts) {
                                            s.usage_deltas = deltas;
                                        }
                                        snapshot
                                    };

                                    if let Err(e) = refresh_token_totals(&db_clone, &state, &app_handle).await {
                                        log::warn!("Failed to refresh token totals: {}", e);
                                    }

                                    let _ = app_handle.emit("usage-updated", &snapshot);
                                    check_notifications(&snapshot, &mut alert_states, &app_handle);
                                }
                                Err(e) => {
                                    if e.contains("account changed") { reconnect = true; continue; }
                                    log::warn!("Failed to refresh rate limits: {}", e);
                                    let _ = db_for_samples.insert_app_event(
                                        "refresh_failed",
                                        "Refresh failed",
                                        Some(&e),
                                    );
                                }
                            }

                            if let Err(e) = refresh_account_usage(c, &db_clone, &state, &app_handle).await {
                                log::warn!("Failed to refresh account token usage: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Persist each window of a snapshot as a quota sample in the database.
fn persist_quota_samples(db: &crate::db::Db, snapshot: &UsageSnapshot) {
    let Some(account_key) = snapshot.account_key.as_deref() else {
        return;
    };
    crate::limit_history::persist_snapshot(db, snapshot);
    let now = chrono::Utc::now().timestamp();

    for window in &snapshot.windows {
        let resets_at_ts = window.resets_at.as_ref().and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.timestamp())
        });

        if let Err(e) = db.insert_quota_sample(
            account_key,
            now,
            &window.name,
            window.duration_minutes,
            window.used_percent,
            window.remaining_percent,
            resets_at_ts,
            &window.source,
        ) {
            log::error!(
                "Failed to persist quota sample for '{}': {}",
                window.name,
                e
            );
        }
    }
}

/// Updates state with a new snapshot while stabilizing `resets_at` when used_percent == 0
/// so that rolling placeholder timestamps from periodic polls don't reset the timer display.
fn update_snapshot_state(s: &mut UsageState, mut new_snapshot: UsageSnapshot) -> UsageSnapshot {
    s.monitor_state = MonitorState::Monitoring;
    s.error_message = None;
    if let Some(existing) = s
        .snapshot
        .as_ref()
        .filter(|old| old.account_key == new_snapshot.account_key)
    {
        for new_win in &mut new_snapshot.windows {
            if new_win.used_percent == 0.0 {
                if let Some(old_win) = existing.windows.iter().find(|w| w.name == new_win.name) {
                    if old_win.used_percent == 0.0
                        && old_win
                            .resets_at
                            .as_deref()
                            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                            .is_some_and(|t| t.timestamp() > chrono::Utc::now().timestamp())
                    {
                        new_win.resets_at = old_win.resets_at.clone();
                    }
                }
            }
        }
        new_snapshot.latest_context_window = existing.latest_context_window;
        new_snapshot.latest_context_load_percent = existing.latest_context_load_percent;
        new_snapshot.latest_last_request_tokens = existing.latest_last_request_tokens.clone();
    }
    s.snapshot = Some(new_snapshot.clone());
    new_snapshot
}

pub(crate) fn token_window_bounds(
    snapshot: Option<&UsageSnapshot>,
    window_name: &str,
    fallback_duration_secs: i64,
    now: i64,
) -> (i64, i64) {
    let window = snapshot.and_then(|snapshot| {
        snapshot.windows.iter().find(|window| {
            window.name == window_name
                || match window_name {
                    "fiveHour" => window.duration_minutes == Some(300),
                    "weekly" => window.duration_minutes == Some(10_080),
                    _ => false,
                }
        })
    });

    if let Some(window) = window {
        if let Some(reset_at) = window.resets_at.as_deref() {
            if let Ok(reset) = chrono::DateTime::parse_from_rfc3339(reset_at) {
                let duration_secs = window
                    .duration_minutes
                    .map(|minutes| minutes as i64 * 60)
                    .unwrap_or(fallback_duration_secs);
                let end = reset.timestamp();
                if end <= now {
                    return (now, now);
                }
                return (end - duration_secs, end);
            }
        }
    }

    (now - fallback_duration_secs, now)
}

/// Recompute locally recorded token totals using the active quota-window boundaries.
pub(crate) async fn refresh_token_totals(
    db: &crate::db::Db,
    state: &RwLock<UsageState>,
    app_handle: &tauri::AppHandle,
) -> Result<(), String> {
    let (snapshot, session_start_ts, account_key) = {
        let s = state.read().await;
        let Some(account) = &s.active_account else {
            return Ok(());
        };
        (
            s.snapshot.clone(),
            s.session_start_ts,
            account.account_key.clone(),
        )
    };
    let now = chrono::Utc::now().timestamp();
    let (five_hour_start, five_hour_end) =
        token_window_bounds(snapshot.as_ref(), "fiveHour", 5 * 3600, now);
    let (weekly_start, weekly_end) =
        token_window_bounds(snapshot.as_ref(), "weekly", 7 * 24 * 3600, now);

    let totals = db.get_token_totals(
        &account_key,
        session_start_ts,
        five_hour_start,
        five_hour_end,
        weekly_start,
        weekly_end,
    )?;

    {
        let mut s = state.write().await;
        if s.active_account.as_ref().map(|a| &a.account_key) != Some(&account_key) {
            return Ok(());
        }
        s.token_totals = totals.clone();
    }
    let _ = app_handle.emit("token-totals-updated", &totals);
    Ok(())
}

async fn refresh_account_usage(
    client: &CodexClient,
    db: &crate::db::Db,
    state: &RwLock<UsageState>,
    app_handle: &tauri::AppHandle,
) -> Result<(), String> {
    let mut usage = client.read_account_usage().await?;
    usage.fetched_at = Some(chrono::Utc::now().timestamp());
    {
        let mut s = state.write().await;
        if usage.account_key.is_none()
            || usage.account_key.as_deref()
                != s.active_account.as_ref().map(|a| a.account_key.as_str())
        {
            return Ok(());
        }
        db.record_account_usage(&usage)?;
        s.account_usage = Some(usage.clone());
    }
    let _ = app_handle.emit("account-usage-updated", &usage);
    Ok(())
}

fn reset_account_state(state: &mut UsageState) {
    state.active_account = None;
    state.snapshot = None;
    state.account_usage = None;
    state.token_totals = Default::default();
    state.usage_deltas = Default::default();
    state.monitor_state = MonitorState::Dormant;
    state.error_message = None;
    state.session_start_ts = chrono::Utc::now().timestamp();
}

async fn emit_accounts(db: &crate::db::Db, state: &RwLock<UsageState>, app: &tauri::AppHandle) {
    let context = crate::accounts::AccountContext {
        active_account: state.read().await.active_account.clone(),
        accounts: db.list_accounts().unwrap_or_default(),
    };
    let _ = app.emit("accounts-updated", context);
}

/// Check and fire desktop notifications at threshold percentages.
/// Each threshold fires only once per Codex session (resets when Codex closes/reopens).
fn check_notifications(
    snapshot: &UsageSnapshot,
    alert_states: &mut Vec<AlertState>,
    app_handle: &tauri::AppHandle,
) {
    for window in &snapshot.windows {
        // Find or create alert state for this window
        let alert = alert_states
            .iter_mut()
            .find(|a| a.window_name == window.name);

        let alert = if let Some(a) = alert {
            a
        } else {
            alert_states.push(AlertState {
                window_name: window.name.clone(),
                ..Default::default()
            });
            alert_states.last_mut().unwrap()
        };

        let remaining = window.remaining_percent;
        let window_label = match window.name.as_str() {
            "fiveHour" => "5-hour",
            "weekly" => "weekly",
            _ => &window.name,
        };

        // Thresholds: 75%, 50%, 25%, 0% — each fires once, never resets mid-session
        if remaining <= 0.0 && !alert.alerted_0 {
            alert.alerted_0 = true;
            alert.alerted_25 = true;
            alert.alerted_50 = true;
            alert.alerted_75 = true;
            send_notification(
                app_handle,
                &format!("Codex {} limit reached", window_label),
                "0% remaining. Waiting for the weekly reset.",
            );
        } else if remaining <= 25.0 && !alert.alerted_25 {
            alert.alerted_25 = true;
            alert.alerted_50 = true;
            alert.alerted_75 = true;
            send_notification(
                app_handle,
                &format!("Codex {} at 25% remaining", window_label),
                &format!("{:.0}% remaining.", remaining),
            );
        } else if remaining <= 50.0 && !alert.alerted_50 {
            alert.alerted_50 = true;
            alert.alerted_75 = true;
            send_notification(
                app_handle,
                &format!("Codex {} at 50% remaining", window_label),
                &format!("{:.0}% remaining.", remaining),
            );
        } else if remaining <= 75.0 && !alert.alerted_75 {
            alert.alerted_75 = true;
            send_notification(
                app_handle,
                &format!("Codex {} at 75% remaining", window_label),
                &format!("{:.0}% remaining.", remaining),
            );
        }
    }
}

/// Send a desktop notification directly from Rust (avoids duplicate-per-window frontend approach)
fn send_notification(app_handle: &tauri::AppHandle, title: &str, body: &str) {
    log::info!("Notification: {} - {}", title, body);
    let _ = app_handle
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex_client::UsageWindow;

    fn snapshot_with_window(window: UsageWindow) -> UsageSnapshot {
        UsageSnapshot {
            account_key: Some(crate::accounts::LEGACY_ACCOUNT.into()),
            captured_at: "2026-08-31T00:00:00Z".to_string(),
            limit_id: Some("codex".to_string()),
            limit_name: None,
            plan_type: Some("plus".to_string()),
            rate_limit_reached_type: None,
            credits: None,
            windows: vec![window],
            limits: Vec::new(),
            latest_context_window: None,
            latest_context_load_percent: None,
            latest_last_request_tokens: None,
        }
    }

    #[test]
    fn derives_token_window_from_reset_and_duration() {
        let reset = 1_800_000_000;
        let snapshot = snapshot_with_window(UsageWindow {
            source: "primary".to_string(),
            name: "fiveHour".to_string(),
            duration_minutes: Some(300),
            used_percent: 12.0,
            remaining_percent: 88.0,
            resets_at: chrono::DateTime::from_timestamp(reset, 0).map(|dt| dt.to_rfc3339()),
        });

        assert_eq!(
            token_window_bounds(Some(&snapshot), "fiveHour", 5 * 3600, reset - 60),
            (reset - 5 * 3600, reset)
        );
    }

    #[test]
    fn falls_back_to_a_rolling_window_when_reset_is_missing() {
        let now = 1_800_000_000;
        assert_eq!(
            token_window_bounds(None, "weekly", 7 * 24 * 3600, now),
            (now - 7 * 24 * 3600, now)
        );
    }

    #[test]
    fn account_change_clears_quota_activity_context_and_totals() {
        let mut state = UsageState::default();
        state.active_account = Some(crate::accounts::identity("a", "user"));
        state.token_totals.all_time_recorded.total_tokens = 1000;
        state.account_usage = Some(Default::default());
        state.usage_deltas.today_delta = 80.0;
        reset_account_state(&mut state);
        assert!(
            state.active_account.is_none()
                && state.snapshot.is_none()
                && state.account_usage.is_none()
        );
        assert_eq!(state.token_totals.all_time_recorded.total_tokens, 0);
        assert_eq!(state.usage_deltas.today_delta, 0.0);
    }

    #[test]
    fn account_quota_is_recorded_without_local_clients_or_invented_tokens() {
        let db = crate::db::Db::test_db(None);
        let mut state = UsageState::default();
        assert!(state.detected_clients.is_empty());
        let snapshot = snapshot_with_window(UsageWindow {
            source: "primary".into(),
            name: "fiveHour".into(),
            duration_minutes: Some(300),
            used_percent: 35.0,
            remaining_percent: 65.0,
            resets_at: chrono::DateTime::from_timestamp(1_800_000_000, 0).map(|dt| dt.to_rfc3339()),
        });
        persist_quota_samples(&db, &snapshot);
        update_snapshot_state(&mut state, snapshot);
        assert_eq!(state.monitor_state, MonitorState::Monitoring);
        let history = db.get_limit_history(None, None, 0, 1_800_000_001).unwrap();
        assert_eq!(history.periods.len(), 1);
        assert_eq!(history.periods[0].used_percent, 35.0);
        assert_eq!(history.periods[0].tokens.total_tokens, 0);
    }
}
