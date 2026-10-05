use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
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
    AccountChanged,
}

/// Normalized usage snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    #[serde(default)]
    pub account_key: Option<String>,
    pub captured_at: String,
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub plan_type: Option<String>,
    pub rate_limit_reached_type: Option<String>,
    pub credits: Option<serde_json::Value>,
    pub windows: Vec<UsageWindow>,
    #[serde(default)]
    pub limits: Vec<LimitBucket>,
    pub latest_context_window: Option<i64>,
    pub latest_context_load_percent: Option<f64>,
    pub latest_last_request_tokens: Option<crate::db::TokenBreakdown>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitBucket {
    pub limit_id: String,
    pub limit_name: Option<String>,
    pub windows: Vec<UsageWindow>,
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
    #[serde(default)]
    pub account_key: Option<String>,
    pub summary: Option<AccountUsageSummary>,
    pub daily_usage_buckets: Option<Vec<DailyUsageBucket>>,
    #[serde(default)]
    pub fetched_at: Option<i64>,
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
    #[serde(
        alias = "windowDurationMins",
        alias = "window_duration_mins",
        alias = "window_minutes"
    )]
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
    #[serde(alias = "limit_id")]
    limit_id: Option<String>,
    #[serde(alias = "limit_name")]
    limit_name: Option<String>,
    #[serde(alias = "plan_type")]
    plan_type: Option<String>,
    #[serde(alias = "rate_limit_reached_type")]
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
    account: Arc<Mutex<Option<crate::accounts::AccountProfile>>>,
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
            account: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the Codex app-server process and begin communication
    pub async fn start(&mut self) -> Result<(), String> {
        let starting_identity = crate::accounts::local_account()?.map(|a| a.account_key);
        let codex_path = find_codex_executable()?;
        log::info!("[CODEX] Found executable: {}", codex_path.display());

        let mut child = Command::new(&codex_path)
            .args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000) // CREATE_NO_WINDOW on Windows
            .spawn()
            .map_err(|e| {
                format!(
                    "Failed to start codex app-server at {}: {}",
                    codex_path.display(),
                    e
                )
            })?;

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
        let account_reader = self.account.clone();
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
                                if method == "account/updated" {
                                    if account_reader.lock().unwrap().take().is_some() {
                                        let _ = event_tx_reader.send(CodexEvent::AccountChanged);
                                    }
                                }
                                if method == "account/rateLimits/updated" {
                                    if let Some(params) = msg.params {
                                        if let Some(snapshot) = params.get("rateLimits") {
                                            if let Ok(mut normalized) = parse_rate_limits(
                                                &serde_json::json!({
                                                    "rateLimits": snapshot,
                                                    "rateLimitsByLimitId": params.get("rateLimitsByLimitId")
                                                }),
                                            ) {
                                                normalized.account_key = account_reader
                                                    .lock()
                                                    .unwrap()
                                                    .as_ref()
                                                    .map(|a| a.account_key.clone());
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
                                            account_key: account_reader
                                                .lock()
                                                .unwrap()
                                                .as_ref()
                                                .map(|a| a.account_key.clone())
                                                .unwrap_or_else(|| {
                                                    crate::accounts::LEGACY_ACCOUNT.into()
                                                }),
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
                        "version": env!("CARGO_PKG_VERSION")
                    }
                })),
            )
            .await?;

        log::info!("[CODEX] Codex app-server initialized: {:?}", init_result);

        // Send initialized notification
        self.notify("initialized", None);
        self.read_account().await?;
        if starting_identity != crate::accounts::local_account()?.map(|a| a.account_key) {
            return Err("Codex account changed while connecting; reconnecting".into());
        }

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
        let account = self
            .account()
            .ok_or("Please sign in to Codex to read account usage")?;
        self.check_identity(&account)?;
        let result = self.request("account/rateLimits/read", None).await?;
        self.check_identity(&account)?;
        let mut snapshot = parse_rate_limits(&result)?;
        snapshot.account_key = Some(account.account_key);
        Ok(snapshot)
    }

    pub fn account(&self) -> Option<crate::accounts::AccountProfile> {
        self.account.lock().unwrap().clone()
    }

    pub(crate) fn has_current_identity(&self) -> bool {
        self.account()
            .is_some_and(|a| self.check_identity(&a).is_ok())
    }

    fn check_identity(&self, account: &crate::accounts::AccountProfile) -> Result<(), String> {
        if self.account().as_ref().map(|a| &a.account_key) != Some(&account.account_key) {
            return Err("Codex account changed; reconnecting".into());
        }
        if !account.account_key.starts_with("chatgpt-email:")
            && crate::accounts::local_account()?
                .as_ref()
                .map(|a| &a.account_key)
                != Some(&account.account_key)
        {
            return Err("Codex account changed; reconnecting".into());
        }
        Ok(())
    }

    pub async fn read_account(&self) -> Result<Option<crate::accounts::AccountProfile>, String> {
        let result = self
            .request(
                "account/read",
                Some(serde_json::json!({"refreshToken":false})),
            )
            .await?;
        let profile = crate::accounts::from_response(&result, crate::accounts::local_account()?)?;
        *self.account.lock().unwrap() = profile.clone();
        Ok(profile)
    }

    /// Fetch authoritative account-level token activity from Codex.
    pub async fn read_account_usage(&self) -> Result<AccountUsage, String> {
        let account = self
            .account()
            .ok_or("Please sign in to Codex to read account usage")?;
        self.check_identity(&account)?;
        let result = self.request("account/usage/read", None).await?;
        self.check_identity(&account)?;
        let mut usage: AccountUsage = serde_json::from_value(result)
            .map_err(|e| format!("Failed to parse account token usage: {}", e))?;
        usage.account_key = Some(account.account_key);
        Ok(usage)
    }

    /// Stop the client
    pub fn stop(&self) {
        if let Some(cmd_tx) = &self.command_tx {
            let _ = cmd_tx.send(ClientCommand::Stop);
        }
    }
}

/// Resolve native binaries rather than npm's .cmd shim, which requires cmd.exe
/// and fails with error 740 when that shell is configured to run as administrator.
fn codex_executable_candidates(appdata: Option<&OsStr>, path: Option<&OsStr>) -> Vec<PathBuf> {
    let mut prefixes = Vec::new();
    if let Some(appdata) = appdata {
        prefixes.push(Path::new(appdata).join("npm"));
    }
    if let Some(path) = path {
        prefixes.extend(std::env::split_paths(path));
    }

    let (platform_package, target) = if cfg!(target_arch = "aarch64") {
        ("codex-win32-arm64", "aarch64-pc-windows-msvc")
    } else {
        ("codex-win32-x64", "x86_64-pc-windows-msvc")
    };
    let mut candidates = Vec::new();
    for prefix in &prefixes {
        let scope = prefix.join("node_modules").join("@openai");
        let package = scope.join("codex");
        // Current npm installs nest the platform package under @openai/codex.
        // Hoisted dependencies and older bundled-vendor releases also work.
        for vendor in [
            package
                .join("node_modules")
                .join("@openai")
                .join(platform_package)
                .join("vendor"),
            scope.join(platform_package).join("vendor"),
            package.join("vendor"),
        ] {
            for binary_dir in ["bin", "codex"] {
                let candidate = vendor.join(target).join(binary_dir).join("codex.exe");
                if !candidates.contains(&candidate) {
                    candidates.push(candidate);
                }
            }
        }
    }
    // Standalone CLI installations remain supported, using an absolute path.
    for prefix in prefixes {
        let candidate = prefix.join("codex.exe");
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
}

fn find_codex_executable() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA");
    let path = std::env::var_os("PATH");
    let mut failures = Vec::new();
    for candidate in codex_executable_candidates(appdata.as_deref(), path.as_deref()) {
        if !candidate.is_file() {
            continue;
        }
        let candidate = match candidate.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                failures.push(format!("{}: {}", candidate.display(), error));
                continue;
            }
        };
        match Command::new(&candidate)
            .arg("--version")
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .output()
        {
            Ok(output)
                if output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .starts_with("codex-cli ") =>
            {
                return Ok(candidate);
            }
            Ok(output) => failures.push(format!(
                "{} did not report a Codex CLI version (exit {})",
                candidate.display(),
                output.status
            )),
            Err(error) => failures.push(format!("{}: {}", candidate.display(), error)),
        }
    }

    let mut message = "No usable Codex CLI executable found. Install or repair it with: npm install -g @openai/codex".to_string();
    if !failures.is_empty() {
        message.push_str(&format!(". {}", failures.join("; ")));
    }
    Err(message)
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
        account_key: None,
        captured_at: chrono::Utc::now().to_rfc3339(),
        limit_id: raw.limit_id.clone(),
        limit_name: raw.limit_name.clone(),
        plan_type: raw.plan_type.clone(),
        rate_limit_reached_type: raw.rate_limit_reached_type.clone(),
        credits: raw.credits.clone(),
        limits: vec![LimitBucket {
            limit_id: raw.limit_id.clone().unwrap_or_else(|| "codex".into()),
            limit_name: raw.limit_name.clone(),
            windows: windows.clone(),
        }],
        windows,
        latest_context_window: None,
        latest_context_load_percent: None,
        latest_last_request_tokens: None,
    }
}

pub(crate) fn parse_rate_limits(value: &serde_json::Value) -> Result<UsageSnapshot, String> {
    let mut buckets = Vec::new();
    if let Some(map) = value.get("rateLimitsByLimitId").and_then(|v| v.as_object()) {
        for (id, bucket) in map {
            if bucket.is_null() {
                continue;
            }
            let mut raw: RawSnapshot =
                serde_json::from_value(bucket.clone()).map_err(|e| e.to_string())?;
            raw.limit_id = Some(id.clone());
            buckets.push(normalize_snapshot(&raw));
        }
    }
    if let Some(legacy) = value.get("rateLimits").filter(|v| !v.is_null()) {
        let raw: RawSnapshot = serde_json::from_value(legacy.clone()).map_err(|e| e.to_string())?;
        let id = raw.limit_id.as_deref().unwrap_or("codex");
        if !buckets
            .iter()
            .any(|b| b.limit_id.as_deref().unwrap_or("codex") == id)
        {
            buckets.push(normalize_snapshot(&raw));
        }
    }
    let selected = buckets
        .iter()
        .find(|b| b.limit_id.as_deref() == Some("codex"))
        .or_else(|| buckets.first())
        .ok_or("Codex returned no rate-limit snapshot")?;
    let mut snapshot = selected.clone();
    snapshot.limits = buckets.into_iter().flat_map(|b| b.limits).collect();
    Ok(snapshot)
}

pub(crate) fn snapshot_from_rollout(
    value: &serde_json::Value,
    timestamp: &str,
) -> Option<UsageSnapshot> {
    let mut snapshot = parse_rate_limits(&serde_json::json!({ "rateLimits": value })).ok()?;
    snapshot.captured_at = timestamp.to_string();
    Some(snapshot)
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
    fn discovers_native_npm_binaries_with_spaces_and_ignores_cmd_shims() {
        let root = std::env::temp_dir().join(format!("codex meter {}", uuid::Uuid::new_v4()));
        let appdata = root.join("AppData").join("Roaming");
        let prefix = appdata.join("npm");
        let scope = prefix.join("node_modules").join("@openai");
        let (platform, target) = if cfg!(target_arch = "aarch64") {
            ("codex-win32-arm64", "aarch64-pc-windows-msvc")
        } else {
            ("codex-win32-x64", "x86_64-pc-windows-msvc")
        };
        let paths = [
            scope
                .join("codex")
                .join("node_modules")
                .join("@openai")
                .join(platform)
                .join("vendor")
                .join(target)
                .join("bin")
                .join("codex.exe"),
            scope
                .join(platform)
                .join("vendor")
                .join(target)
                .join("bin")
                .join("codex.exe"),
            scope
                .join("codex")
                .join("vendor")
                .join(target)
                .join("codex")
                .join("codex.exe"),
        ];
        std::fs::create_dir_all(&prefix).unwrap();
        std::fs::write(prefix.join("codex.cmd"), "@echo this shim must not run").unwrap();
        for native in &paths {
            std::fs::create_dir_all(native.parent().unwrap()).unwrap();
            std::fs::write(native, []).unwrap();
            let candidates = codex_executable_candidates(Some(appdata.as_os_str()), None);
            assert_eq!(candidates.iter().find(|path| path.is_file()), Some(native));
            assert!(candidates
                .iter()
                .all(|path| path.extension() == Some(OsStr::new("exe"))));
            std::fs::remove_file(native).unwrap();
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires the official Codex CLI; exercised by the Windows build workflow"]
    async fn initializes_native_app_server_without_command_prompt() {
        let executable = find_codex_executable().expect("native Codex CLI should be installed");
        assert_eq!(executable.extension(), Some(OsStr::new("exe")));
        assert!(executable.is_absolute());
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let mut client = CodexClient::new(event_tx);
        client
            .start()
            .await
            .expect("native app-server should initialize");
        client.stop();
        timeout(Duration::from_secs(15), async {
            while let Some(event) = event_rx.recv().await {
                if matches!(event, CodexEvent::Disconnected) {
                    return;
                }
            }
        })
        .await
        .expect("app-server should exit after stdin closes");
    }

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

#[cfg(test)]
mod bucket_tests {
    use super::*;
    #[test]
    fn map_reads_every_bucket_and_deduplicates_legacy() {
        let result = serde_json::json!({"rateLimitsByLimitId": {
            "codex": {"primary": {"windowDurationMins": 300, "usedPercent": 10, "resetsAt": 1800000000}},
            "new-model": {"limitName": "New model", "secondary": {"windowDurationMins": 10080, "usedPercent": 30, "resetsAt": 1800000000}}
        }, "rateLimits": {"limitId": "codex", "primary": {"windowDurationMins": 300, "usedPercent": 10, "resetsAt": 1800000000}}});
        let snapshot = parse_rate_limits(&result).unwrap();
        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.limits.len(), 2);
        assert_eq!(
            snapshot
                .limits
                .iter()
                .find(|b| b.limit_id == "new-model")
                .unwrap()
                .windows[0]
                .name,
            "weekly"
        );
    }
    #[test]
    fn accepts_rollout_snake_case_schema() {
        let snapshot = snapshot_from_rollout(
            &serde_json::json!({"limit_id": "new-coder", "limit_name": "New coder",
            "primary": {"window_minutes": 300, "used_percent": 15, "resets_at": 1800000000}}),
            "2026-10-02T12:00:00Z",
        )
        .unwrap();
        assert_eq!(snapshot.limits[0].limit_id, "new-coder");
        assert_eq!(snapshot.windows[0].name, "fiveHour");
    }
}
