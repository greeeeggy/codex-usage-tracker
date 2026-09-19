use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout, Duration};

/// Events emitted by the Codex client
#[derive(Debug, Clone)]
pub enum CodexEvent {
    UsageUpdated(UsageSnapshot),
    TokenUsageUpdated(crate::db::TokenEvent),
    // Error(String),
    Disconnected,
}

/// Normalized usage snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub captured_at: String,
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub plan_type: Option<String>,
    pub rate_limit_reached_type: Option<String>,
    pub credits: Option<serde_json::Value>,
    pub windows: Vec<UsageWindow>,
    pub latest_context_window: Option<i64>,
    pub latest_context_load_percent: Option<f64>,
    pub latest_last_request_tokens: Option<crate::db::TokenBreakdown>,
}

/// A single rate-limit window (e.g., 5-hour or weekly)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub source: String,
    pub name: String,
    pub duration_minutes: Option<u64>,
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub resets_at: Option<String>,
}

/// Account-level token activity returned by `account/usage/read`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    pub summary: Option<AccountUsageSummary>,
    pub daily_usage_buckets: Option<Vec<DailyUsageBucket>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsageSummary {
    pub lifetime_tokens: Option<i64>,
    pub peak_daily_tokens: Option<i64>,
    pub longest_running_turn_sec: Option<i64>,
    pub current_streak_days: Option<i64>,
    pub longest_streak_days: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsageBucket {
    pub start_date: String,
    pub tokens: i64,
}

/// Raw JSON-RPC message
#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<serde_json::Value>,
}

/// Raw rate-limit window from Codex
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWindow {
    #[serde(alias = "windowDurationMins", alias = "window_duration_mins")]
    window_duration_mins: Option<u64>,
    #[serde(alias = "usedPercent", alias = "used_percent")]
    used_percent: Option<f64>,
    #[serde(alias = "remainingPercent", alias = "remaining_percent")]
    remaining_percent: Option<f64>,
    #[serde(alias = "resetsAt", alias = "resets_at")]
    resets_at: Option<f64>,
}

/// Raw rate-limit snapshot from Codex
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSnapshot {
    limit_id: Option<String>,
    limit_name: Option<String>,
    plan_type: Option<String>,
    rate_limit_reached_type: Option<String>,
    credits: Option<serde_json::Value>,
    primary: Option<RawWindow>,
    secondary: Option<RawWindow>,
}

/// Pending request awaiting a response
struct PendingRequest {
    sender: oneshot::Sender<Result<serde_json::Value, String>>,
}

type PendingMap = Arc<Mutex<HashMap<u64, PendingRequest>>>;

pub struct CodexClient {
    event_tx: mpsc::UnboundedSender<CodexEvent>,
    command_tx: Option<mpsc::UnboundedSender<ClientCommand>>,
}

enum ClientCommand {
    SendRequest {
        id: u64,
        method: String,
        params: Option<serde_json::Value>,
        response_tx: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    SendNotification {
        method: String,
        params: Option<serde_json::Value>,
    },
    Stop,
}

impl CodexClient {
    pub fn new(event_tx: mpsc::UnboundedSender<CodexEvent>) -> Self {
        Self {
            event_tx,
            command_tx: None,
        }
    }

    /// Start the Codex app-server process and begin communication
    pub async fn start(&mut self) -> Result<(), String> {
        let codex_path = find_codex_executable()?;
        log::info!("[CODEX] Found executable: {}", codex_path);

        let mut child = Command::new(&codex_path)
            .args(["app-server", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000) // CREATE_NO_WINDOW on Windows
            .spawn()
            .map_err(|e| format!("Failed to start codex app-server: {}", e))?;

        log::info!("[CODEX] Spawned codex app-server (PID: {})", child.id());

        let stdin = child.stdin.take().ok_or("Failed to get stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to get stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to get stderr")?;

        let _next_id = Arc::new(AtomicU64::new(1));
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));

        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<ClientCommand>();
        self.command_tx = Some(cmd_tx);

        // Stderr reader thread
        let event_tx_err = self.event_tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(text) => {
                        if !text.trim().is_empty() {
                            log::debug!("[codex stderr] {}", text);
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = event_tx_err; // keep alive
        });

        // Stdout reader thread - reads JSON lines and dispatches
        let pending_reader = pending.clone();
        let event_tx_reader = self.event_tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(text) => {
                        if text.trim().is_empty() {
                            continue;
                        }
                        log::debug!("[CODEX STDOUT] {}", &text[..text.len().min(500)]);
                        if let Ok(msg) = serde_json::from_str::<JsonRpcMessage>(&text) {
                            // Response to a request
                            if let Some(id) = msg.id {
                                if msg.method.is_none() {
                                    let mut map = pending_reader.lock().unwrap();
                                    if let Some(req) = map.remove(&id) {
                                        if let Some(error) = msg.error {
                                            let err_msg = error
                                                .get("message")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("Unknown error")
                                                .to_string();
                                            let _ = req.sender.send(Err(err_msg));
                                        } else {
                                            let _ = req.sender.send(Ok(msg
                                                .result
                                                .unwrap_or(serde_json::Value::Null)));
                                        }
                                    }
                                    continue;
                                }
                            }

                            // Push notification
                            if let Some(method) = &msg.method {
                                if method == "account/rateLimits/updated" {
                                    if let Some(params) = msg.params {
                                        if let Some(snapshot) = params.get("rateLimits") {
                                            if let Ok(raw) = serde_json::from_value::<RawSnapshot>(
                                                snapshot.clone(),
                                            ) {
                                                let normalized = normalize_snapshot(&raw);
                                                let _ = event_tx_reader
                                                    .send(CodexEvent::UsageUpdated(normalized));
                                            }
                                        }
                                    }
                                } else if method == "thread/tokenUsage/updated" {
                                    if let Some(params) = msg.params {
                                        // Current app-server notifications wrap the request
                                        // breakdown in tokenUsage.last and the cumulative
                                        // breakdown in tokenUsage.total.
                                        let token_usage =
                                            params.get("tokenUsage").unwrap_or(&params);
                                        let usage_params =
                                            token_usage.get("last").unwrap_or(token_usage);
                                        let input_tokens = usage_params
                                            .get("inputTokens")
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(0);
                                        let cached_input_tokens = usage_params
                                            .get("cachedInputTokens")
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(0);
                                        let output_tokens = usage_params
                                            .get("outputTokens")
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(0);
                                        let reasoning_tokens = usage_params
                                            .get("reasoningOutputTokens")
                                            .or_else(|| usage_params.get("reasoningTokens"))
                                            .and_then(|v| v.as_i64());
                                        let total_tokens = usage_params
                                            .get("totalTokens")
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(input_tokens + output_tokens);

                                        let event = crate::db::TokenEvent {
                                            id: uuid::Uuid::new_v4().to_string(),
                                            account_key: "default".to_string(),
                                            captured_at: chrono::Utc::now().timestamp(),
                                            client_type: "unknown".to_string(),
                                            thread_id: params
                                                .get("threadId")
                                                .and_then(|v| v.as_str())
                                                .map(|s| s.to_string()),
                                            turn_id: params
                                                .get("turnId")
                                                .and_then(|v| v.as_str())
                                                .map(|s| s.to_string()),
                                            project_path_hash: None,
                                            model: None,
                                            input_tokens,
                                            cached_input_tokens,
                                            output_tokens,
                                            reasoning_tokens,
                                            total_tokens,
                                            event_type: "turn_final".to_string(),
                                        };

                                        let _ = event_tx_reader
                                            .send(CodexEvent::TokenUsageUpdated(event));
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = event_tx_reader.send(CodexEvent::Disconnected);
        });

        // Writer task - receives commands and writes to stdin
        let pending_writer = pending.clone();
        let stdin_mutex = Arc::new(Mutex::new(stdin));
        let stdin_write = stdin_mutex.clone();

        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    ClientCommand::SendRequest {
                        id,
                        method,
                        params,
                        response_tx,
                    } => {
                        let msg = JsonRpcMessage {
                            id: Some(id),
                            method: Some(method),
                            params,
                            result: None,
                            error: None,
                        };

                        {
                            let mut map = pending_writer.lock().unwrap();
                            map.insert(
                                id,
                                PendingRequest {
                                    sender: response_tx,
                                },
                            );
                        }

                        if let Ok(json) = serde_json::to_string(&msg) {
                            let mut writer = stdin_write.lock().unwrap();
                            let _ = writeln!(writer, "{}", json);
                            let _ = writer.flush();
                        }
                    }
                    ClientCommand::SendNotification { method, params } => {
                        let msg = JsonRpcMessage {
                            id: None,
                            method: Some(method),
                            params,
                            result: None,
                            error: None,
                        };

                        if let Ok(json) = serde_json::to_string(&msg) {
                            let mut writer = stdin_write.lock().unwrap();
                            let _ = writeln!(writer, "{}", json);
                            let _ = writer.flush();
                        }
                    }
                    ClientCommand::Stop => break,
                }
            }
        });

        // Initialize the connection
        let init_result = self
            .request(
                "initialize",
                Some(serde_json::json!({
                    "clientInfo": {
                        "name": "codex_meter",
                        "title": "Codex Meter",
                        "version": "0.1.0"
                    }
                })),
            )
            .await?;

        log::info!("[CODEX] Codex app-server initialized: {:?}", init_result);

        // Send initialized notification
        self.notify("initialized", None);

        Ok(())
    }

    /// Send a request and wait for a response
    pub async fn request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let cmd_tx = self.command_tx.as_ref().ok_or("Client not started")?;

        let (response_tx, response_rx) = oneshot::channel();

        cmd_tx
            .send(ClientCommand::SendRequest {
                id: rand_id(),
                method: method.to_string(),
                params,
                response_tx,
            })
            .map_err(|_| "Failed to send command")?;

        match timeout(Duration::from_secs(15), response_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("Response channel dropped".to_string()),
            Err(_) => Err(format!("Request timed out: {}", method)),
        }
    }

    /// Send a notification (no response expected)
    pub fn notify(&self, method: &str, params: Option<serde_json::Value>) {
        if let Some(cmd_tx) = &self.command_tx {
            let _ = cmd_tx.send(ClientCommand::SendNotification {
                method: method.to_string(),
                params,
            });
        }
    }

    /// Fetch current rate limits
    pub async fn read_rate_limits(&self) -> Result<UsageSnapshot, String> {
        let result = self.request("account/rateLimits/read", None).await?;
        log::info!(
            "[CODEX] Raw rate limits response: {}",
            serde_json::to_string_pretty(&result).unwrap_or_default()
        );

        // Try to extract rate limits from the response
        let snapshot_value = result
            .get("rateLimitsByLimitId")
            .and_then(|v| v.get("codex"))
            .or_else(|| result.get("rateLimits"))
            .ok_or("Codex returned no rate-limit snapshot")?;

        log::info!(
            "[CODEX] Extracted snapshot value: {}",
            serde_json::to_string_pretty(&snapshot_value).unwrap_or_default()
        );

        let raw: RawSnapshot = serde_json::from_value(snapshot_value.clone())
            .map_err(|e| format!("Failed to parse rate limits: {}", e))?;

        log::info!(
            "[CODEX] Parsed raw snapshot: planType={:?}, primary={:?}, secondary={:?}",
            raw.plan_type,
            raw.primary.is_some(),
            raw.secondary.is_some()
        );

        Ok(normalize_snapshot(&raw))
    }

    /// Fetch authoritative account-level token activity from Codex.
    pub async fn read_account_usage(&self) -> Result<AccountUsage, String> {
        let result = self.request("account/usage/read", None).await?;
        serde_json::from_value(result)
            .map_err(|e| format!("Failed to parse account token usage: {}", e))
    }

    /// Stop the client
    pub fn stop(&self) {
        if let Some(cmd_tx) = &self.command_tx {
            let _ = cmd_tx.send(ClientCommand::Stop);
        }
    }
}

/// Find the codex executable on the system
fn find_codex_executable() -> Result<String, String> {
    // Try "codex" directly (should be on PATH if installed globally)
    if Command::new("codex")
        .arg("--version")
        .creation_flags(0x08000000)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
    {
        return Ok("codex".to_string());
    }

    // Try common npm global paths on Windows
    if let Ok(appdata) = std::env::var("APPDATA") {
        let npm_path = format!("{}\\npm\\codex.cmd", appdata);
        if std::path::Path::new(&npm_path).exists() {
            return Ok(npm_path);
        }
    }

    Err("Codex CLI not found. Please install it with: npm install -g @openai/codex".to_string())
}

/// Generate a simple incrementing ID
fn rand_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Normalize the raw snapshot into our clean format
fn normalize_snapshot(raw: &RawSnapshot) -> UsageSnapshot {
    let mut windows = Vec::new();

    if let Some(primary) = &raw.primary {
        windows.push(normalize_window("primary", primary));
    }

    if let Some(secondary) = &raw.secondary {
        windows.push(normalize_window("secondary", secondary));
    }

    UsageSnapshot {
        captured_at: chrono::Utc::now().to_rfc3339(),
        limit_id: raw.limit_id.clone(),
        limit_name: raw.limit_name.clone(),
        plan_type: raw.plan_type.clone(),
        rate_limit_reached_type: raw.rate_limit_reached_type.clone(),
        credits: raw.credits.clone(),
        windows,
        latest_context_window: None,
        latest_context_load_percent: None,
        latest_last_request_tokens: None,
    }
}

/// Normalize a single window
fn normalize_window(source: &str, raw: &RawWindow) -> UsageWindow {
    let (used, remaining) = if let Some(r) = raw.remaining_percent {
        let rem_clamped = r.clamp(0.0, 100.0);
        (100.0 - rem_clamped, rem_clamped)
    } else if let Some(u) = raw.used_percent {
        let u_clamped = u.clamp(0.0, 100.0);
        let rem_clamped = (100.0 - u_clamped).clamp(0.0, 100.0);
        (u_clamped, rem_clamped)
    } else {
        (0.0, 100.0)
    };

    let resets_at = raw.resets_at.map(|ts| {
        chrono::DateTime::from_timestamp(ts as i64, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default()
    });

    let name = match raw.window_duration_mins {
        Some(300) => "fiveHour".to_string(),
        Some(10080) => "weekly".to_string(),
        Some(mins) => format!("{}Minutes", mins),
        None => "unknown".to_string(),
    };

    UsageWindow {
        source: source.to_string(),
        name,
        duration_minutes: raw.window_duration_mins,
        used_percent: used,
        remaining_percent: remaining,
        resets_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_five_hour_and_weekly_windows() {
        let raw = RawSnapshot {
            limit_id: Some("codex".to_string()),
            limit_name: None,
            plan_type: Some("plus".to_string()),
            rate_limit_reached_type: None,
            credits: None,
            primary: Some(RawWindow {
                window_duration_mins: Some(300),
                used_percent: Some(8.0),
                remaining_percent: None,
                resets_at: Some(1_800_000_000.0),
            }),
            secondary: Some(RawWindow {
                window_duration_mins: Some(10_080),
                used_percent: Some(11.0),
                remaining_percent: None,
                resets_at: Some(1_800_604_800.0),
            }),
        };

        let snapshot = normalize_snapshot(&raw);
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].name, "fiveHour");
        assert_eq!(snapshot.windows[0].remaining_percent, 92.0);
        assert_eq!(snapshot.windows[1].name, "weekly");
        assert_eq!(snapshot.windows[1].remaining_percent, 89.0);
    }

    #[test]
    fn parses_account_usage_response() {
        let usage: AccountUsage = serde_json::from_value(serde_json::json!({
            "summary": {
                "lifetimeTokens": 1234567,
                "peakDailyTokens": 45678,
                "longestRunningTurnSec": 540,
                "currentStreakDays": 8,
                "longestStreakDays": 14
            },
            "dailyUsageBuckets": [
                { "startDate": "2026-08-30", "tokens": 12345 }
            ],
            "threadUsage": null
        }))
        .expect("account usage should deserialize");

        assert_eq!(
            usage.summary.and_then(|summary| summary.lifetime_tokens),
            Some(1_234_567)
        );
        assert_eq!(
            usage
                .daily_usage_buckets
                .and_then(|buckets| buckets.first().map(|bucket| bucket.tokens)),
            Some(12_345)
        );
    }
}
