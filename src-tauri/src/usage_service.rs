use crate::codex_client::{CodexClient, CodexEvent, UsageSnapshot};
use crate::process_detector;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Emitter, Listener};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{Duration, Instant};

/// Monitor state machine states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MonitorState {
    Dormant,
    Connecting,
    Monitoring,
    GracePeriod,
    AuthRequired,
    Error,
}

/// Alert state for notification thresholds — resets each time Codex is reopened
#[derive(Debug, Clone)]
struct AlertState {
    window_name: String,
    alerted_75: bool,
    alerted_50: bool,
    alerted_25: bool,
    alerted_0: bool,
}

impl Default for AlertState {
    fn default() -> Self {
        Self {
            window_name: String::new(),
            alerted_75: false,
            alerted_50: false,
            alerted_25: false,
            alerted_0: false,
        }
    }
}

/// Shared usage state accessible from Tauri commands
#[derive(Debug, Clone, Default)]
pub struct UsageState {
    pub snapshot: Option<UsageSnapshot>,
    pub monitor_state: MonitorState,
    pub error_message: Option<String>,
    pub detected_clients: Vec<process_detector::DetectedClient>,
    pub token_totals: crate::db::TokenTotals,
}

impl Default for MonitorState {
    fn default() -> Self {
        MonitorState::Dormant
    }
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
        Self {
            state: Arc::new(RwLock::new(UsageState::default())),
            app_handle,
            db,
        }
    }

    /// Get a reference to the shared state
    pub fn state(&self) -> Arc<RwLock<UsageState>> {
        self.state.clone()
    }

    /// Start the monitoring loop
    pub async fn run(self) {
        let state = self.state.clone();
        let app_handle = self.app_handle.clone();
        let db_clone = self.db.clone();

        let mut grace_period_start: Option<Instant> = None;
        let mut client: Option<CodexClient> = None;
        let mut refresh_interval = tokio::time::interval(Duration::from_secs(30));
        let mut process_check_interval = tokio::time::interval(Duration::from_secs(3));
        let mut alert_states: Vec<AlertState> = Vec::new();

        // Channel for receiving events from the Codex client
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<CodexEvent>();

        // Internal channel for manual refresh
        let (internal_tx, mut internal_rx) = mpsc::unbounded_channel::<()>();
        app_handle.listen("refresh-requested", move |_| {
            let _ = internal_tx.send(());
        });

        loop {
            tokio::select! {
                // Process detection tick
                _ = process_check_interval.tick() => {
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
                        (MonitorState::Dormant, true) => {
                            log::info!("Codex client detected, starting monitoring...");
                            {
                                let mut s = state.write().await;
                                s.monitor_state = MonitorState::Connecting;
                                s.error_message = None;
                            }
                            let _ = app_handle.emit("state-changed", "connecting");

                            // Start Codex client
                            let mut new_client = CodexClient::new(event_tx.clone());
                            log::info!("[CONNECT] Attempting to start Codex client...");
                            match new_client.start().await {
                                Ok(_) => {
                                    log::info!("[CONNECT] Connected to Codex app-server successfully");

                                    // Fetch initial usage
                                    log::info!("[DATA] Fetching initial rate limits...");
                                    match new_client.read_rate_limits().await {
                                        Ok(snapshot) => {
                                            log::info!("[DATA] Got snapshot: plan={:?}, windows={}", snapshot.plan_type, snapshot.windows.len());
                                            for w in &snapshot.windows {
                                                log::info!("[DATA]   window '{}': used={:.1}% remaining={:.1}% resets_at={:?}", w.name, w.used_percent, w.remaining_percent, w.resets_at);
                                            }

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
                                                "Codex is now open",
                                                &status_parts.join(" · "),
                                            );

                                            let mut s = state.write().await;
                                            let snapshot = update_snapshot_state(&mut s, snapshot);
                                            s.monitor_state = MonitorState::Monitoring;
                                            let _ = app_handle.emit("usage-updated", &snapshot);
                                            let _ = app_handle.emit("state-changed", "monitoring");
                                        }
                                        Err(e) => {
                                            log::error!("[DATA] FAILED to read initial rate limits: {}", e);
                                            let mut s = state.write().await;
                                            s.monitor_state = MonitorState::Monitoring;
                                            let _ = app_handle.emit("state-changed", "monitoring");
                                        }
                                    }

                                    client = Some(new_client);
                                }
                                Err(e) => {
                                    log::error!("[CONNECT] FAILED to start Codex client: {}", e);
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

                        (MonitorState::Monitoring, false) => {
                            log::info!("No Codex clients detected, entering grace period...");
                            grace_period_start = Some(Instant::now());
                            {
                                let mut s = state.write().await;
                                s.monitor_state = MonitorState::GracePeriod;
                            }
                            let _ = app_handle.emit("state-changed", "gracePeriod");
                        }

                        (MonitorState::GracePeriod, true) => {
                            log::info!("Codex client re-detected, resuming monitoring");
                            grace_period_start = None;
                            {
                                let mut s = state.write().await;
                                s.monitor_state = MonitorState::Monitoring;
                            }
                            let _ = app_handle.emit("state-changed", "monitoring");
                        }

                        (MonitorState::GracePeriod, false) => {
                            if let Some(start) = grace_period_start {
                                if start.elapsed() > Duration::from_secs(60) {
                                    log::info!("Grace period expired, going dormant");
                                    grace_period_start = None;

                                    if let Some(c) = client.take() {
                                        c.stop();
                                    }

                                    {
                                        let mut s = state.write().await;
                                        s.monitor_state = MonitorState::Dormant;
                                    }
                                    alert_states.clear();
                                    let _ = app_handle.emit("state-changed", "dormant");
                                }
                            }
                        }

                        (MonitorState::Error, true) => {
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
                    let current_state = {
                        state.read().await.monitor_state.clone()
                    };

                    if current_state == MonitorState::Monitoring {
                        if let Some(c) = &client {
                            match c.read_rate_limits().await {
                                Ok(snapshot) => {
                                    let mut s = state.write().await;
                                    let snapshot = update_snapshot_state(&mut s, snapshot);
                                    let _ = app_handle.emit("usage-updated", &snapshot);

                                    // Check notification thresholds
                                    check_notifications(&snapshot, &mut alert_states, &app_handle);
                                }
                                Err(e) => {
                                    log::warn!("Failed to refresh rate limits: {}", e);
                                }
                            }
                        }
                    }
                }

                // Events from Codex client (push notifications)
                Some(event) = event_rx.recv() => {
                    match event {
                        CodexEvent::UsageUpdated(snapshot) => {
                            log::info!("Received push usage update");
                            let mut s = state.write().await;
                            let snapshot = update_snapshot_state(&mut s, snapshot);
                            let _ = app_handle.emit("usage-updated", &snapshot);

                            check_notifications(&snapshot, &mut alert_states, &app_handle);
                        }
                        CodexEvent::TokenUsageUpdated(token_event) => {
                            log::info!("Received token usage update: {} tokens", token_event.total_tokens);
                            if let Err(e) = db_clone.insert_token_event(&token_event) {
                                log::error!("Failed to persist token event: {}", e);
                            } else {
                                let current_state = state.read().await;
                                if let Some(snapshot) = &current_state.snapshot {
                                    let mut five_hour_start = 0;
                                    let mut five_hour_end = i64::MAX;
                                    let mut weekly_start = 0;
                                    let mut weekly_end = i64::MAX;

                                    for w in &snapshot.windows {
                                        if w.name == "fiveHour" {
                                            if let Some(resets) = &w.resets_at {
                                                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(resets) {
                                                    five_hour_end = dt.timestamp();
                                                    five_hour_start = five_hour_end - 5 * 3600;
                                                }
                                            }
                                        } else if w.name == "weekly" {
                                            if let Some(resets) = &w.resets_at {
                                                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(resets) {
                                                    weekly_end = dt.timestamp();
                                                    weekly_start = weekly_end - 7 * 24 * 3600;
                                                }
                                            }
                                        }
                                    }

                                    match db_clone.get_token_totals(
                                        &token_event.account_key,
                                        0,
                                        five_hour_start,
                                        five_hour_end,
                                        weekly_start,
                                        weekly_end,
                                    ) {
                                        Ok(totals) => {
                                            drop(current_state);
                                            let mut s = state.write().await;
                                            s.token_totals = totals.clone();
                                            let _ = app_handle.emit("token-totals-updated", &totals);
                                        }
                                        Err(e) => log::error!("Failed to calculate token totals: {}", e),
                                    }
                                }
                            }
                        }
                        // CodexEvent::Error(e) => {
                        //     log::error!("Codex client error: {}", e);
                        // }
                        CodexEvent::Disconnected => {
                            log::warn!("Codex app-server disconnected");
                            client = None;
                            let mut s = state.write().await;
                            if s.monitor_state == MonitorState::Monitoring {
                                s.monitor_state = MonitorState::Dormant;
                                s.error_message = None;
                                alert_states.clear();
                                let _ = app_handle.emit("state-changed", "dormant");
                            }
                        }
                    }
                }

                // Internal events
                Some(_) = internal_rx.recv() => {
                    log::info!("Manual refresh requested");
                    if state.read().await.monitor_state == MonitorState::Monitoring {
                        if let Some(c) = &client {
                            match c.read_rate_limits().await {
                                Ok(snapshot) => {
                                    let mut s = state.write().await;
                                    let snapshot = update_snapshot_state(&mut s, snapshot);
                                    let _ = app_handle.emit("usage-updated", &snapshot);
                                    check_notifications(&snapshot, &mut alert_states, &app_handle);
                                }
                                Err(e) => {
                                    log::warn!("Failed to refresh rate limits: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Updates state with a new snapshot while stabilizing `resets_at` when used_percent == 0
/// so that rolling placeholder timestamps from periodic polls don't reset the timer display.
fn update_snapshot_state(s: &mut UsageState, mut new_snapshot: UsageSnapshot) -> UsageSnapshot {
    if let Some(existing) = &s.snapshot {
        for new_win in &mut new_snapshot.windows {
            if new_win.used_percent == 0.0 {
                if let Some(old_win) = existing.windows.iter().find(|w| w.name == new_win.name) {
                    if old_win.used_percent == 0.0 && old_win.resets_at.is_some() {
                        new_win.resets_at = old_win.resets_at.clone();
                    }
                }
            }
        }
    }
    s.snapshot = Some(new_snapshot.clone());
    new_snapshot
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
