//! Transparent desktop app-server bridge. Only its private RPCs touch authentication.
//! Ordinary desktop traffic and notifications are preserved, with no transcript logging.
use crate::{account_switch as vault, accounts::AccountProfile};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot, RwLock},
    time::{timeout, Duration},
};

pub fn meter_root() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("com.codexmeter.app"))
        .ok_or("Cannot locate local Meter storage".into())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Configuration {
    upstream: PathBuf,
    #[serde(default)]
    upstream_args: Vec<String>,
    launcher: PathBuf,
    previous_launcher: Option<String>,
    enabled: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection {
    pid: u32,
    port: u16,
    home: PathBuf,
    capability: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveStatus {
    pub enabled: bool,
    pub connected: usize,
    pub busy: bool,
    pub runtime_account: Option<AccountProfile>,
}
fn configuration() -> Result<Configuration, String> {
    serde_json::from_slice(
        &fs::read(meter_root()?.join("live-switch.json"))
            .map_err(|_| "Enable live switching in Meter first")?,
    )
    .map_err(|_| "Live-switch setup is damaged; enable it again".into())
}
fn find_upstream() -> Result<PathBuf, String> {
    let bin = std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("OpenAI/Codex/bin"));
    if let Some(bin) = &bin {
        let mut system = sysinfo::System::new();
        system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        if let Some(path) = system
            .processes()
            .values()
            .filter_map(|p| p.exe())
            .find(|p| {
                p.starts_with(bin)
                    && p.file_name()
                        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("codex.exe"))
            })
        {
            return Ok(path.to_path_buf());
        }
        let mut candidates: Vec<_> = fs::read_dir(bin)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.path().join("codex.exe"))
            .filter(|p| p.is_file())
            .collect();
        candidates.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
        if let Some(path) = candidates.pop() {
            return Ok(path);
        }
    }
    crate::codex_client::find_codex_executable()
}
fn upstream() -> Result<(PathBuf, Vec<String>), String> {
    let config = configuration()?;
    if config.upstream.is_file() && config.upstream != config.launcher {
        Ok((config.upstream, config.upstream_args))
    } else {
        Ok((find_upstream()?, vec![]))
    }
}
fn environment(action: &str, value: Option<&str>) -> Result<Option<String>, String> {
    use std::os::windows::process::CommandExt;
    let script = r#"$ErrorActionPreference='Stop'; $r=[Console]::In.ReadToEnd()|ConvertFrom-Json; $previous=[Environment]::GetEnvironmentVariable('CODEX_CLI_PATH','User'); if($r.action -eq 'set'){[Environment]::SetEnvironmentVariable('CODEX_CLI_PATH',$r.value,'User'); Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class MeterEnv { [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr hwnd,uint msg,UIntPtr w,string l,uint flags,uint timeout,out UIntPtr result); }'; $result=[UIntPtr]::Zero; $null=[MeterEnv]::SendMessageTimeout([IntPtr]0xffff,0x1a,[UIntPtr]::Zero,'Environment',2,1000,[ref]$result)}; @{previous=$previous}|ConvertTo-Json -Compress"#;
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Cannot update the Codex launcher")?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            json!({"action":action,"value":value})
                .to_string()
                .as_bytes(),
        )
        .map_err(|_| "Cannot update the Codex launcher")?;
    let result = child
        .wait_with_output()
        .map_err(|_| "Cannot update the Codex launcher")?;
    if !result.status.success() {
        return Err("Windows could not update the Codex launcher setting".into());
    }
    let result: Value =
        serde_json::from_slice(&result.stdout).map_err(|_| "Invalid launcher response")?;
    Ok(result["previous"].as_str().map(str::to_owned))
}
#[tauri::command]
pub async fn configure_live_switching(
    manager: tauri::State<'_, vault::SwitchManager>,
    enabled: bool,
) -> Result<LiveStatus, String> {
    let _guard = manager
        .operation
        .try_lock()
        .map_err(|_| "An account operation is already in progress")?;
    let current = std::env::current_exe().map_err(|_| "Cannot locate Meter")?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = meter_root()?;
        if enabled {
            let old = configuration().ok();
            let launcher = root.join("bridge/Codex-Meter-Bridge.exe");
            let upstream = find_upstream()?;
            fs::create_dir_all(launcher.parent().unwrap())
                .map_err(|_| "Cannot install the desktop bridge")?;
            if !launcher.exists() || fs::read(&launcher).ok() != fs::read(&current).ok() {
                fs::copy(&current, &launcher).map_err(|_| {
                    "Quit Codex normally once before updating the live-switch bridge"
                })?;
            }
            let previous = environment("get", None)?;
            let original = if previous.as_deref() == launcher.to_str() {
                old.and_then(|c| c.previous_launcher)
            } else {
                previous
            };
            let config = Configuration {
                upstream,
                upstream_args: vec![],
                launcher: launcher.clone(),
                previous_launcher: original,
                enabled: true,
            };
            vault::atomic_write(
                &root.join("live-switch.json"),
                &serde_json::to_vec_pretty(&config).unwrap(),
            )?;
            environment("set", launcher.to_str())?;
        } else if let Ok(mut config) = configuration() {
            let value = environment("get", None)?;
            if value.as_deref() == config.launcher.to_str() {
                environment("set", config.previous_launcher.as_deref())?;
            }
            config.enabled = false;
            vault::atomic_write(
                &root.join("live-switch.json"),
                &serde_json::to_vec_pretty(&config).unwrap(),
            )?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|_| "Could not configure live switching")??;
    status().await
}
fn connections() -> Result<Vec<(Connection, String)>, String> {
    let config = configuration()?;
    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let expected_home = vault::codex_home()?
        .canonicalize()
        .map_err(|_| "Cannot locate the active Codex home")?;
    let mut rows = vec![];
    for entry in fs::read_dir(meter_root()?.join("bridge/connections"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
    {
        let Ok(bytes) = fs::read(entry.path()) else {
            continue;
        };
        let Ok(row) = serde_json::from_slice::<Connection>(&bytes) else {
            continue;
        };
        if row.home != expected_home
            || !system
                .process(sysinfo::Pid::from_u32(row.pid))
                .and_then(|p| p.exe())
                .is_some_and(|exe| {
                    exe.canonicalize()
                        .ok()
                        .zip(config.launcher.canonicalize().ok())
                        .is_some_and(|(a, b)| a == b)
                })
        {
            continue;
        }
        let Ok(ciphertext) = base64::engine::general_purpose::STANDARD.decode(&row.capability)
        else {
            continue;
        };
        let Ok(capability) = vault::crypt(&ciphertext, false)
            .and_then(|b| String::from_utf8(b).map_err(|_| "Invalid capability".into()))
        else {
            continue;
        };
        rows.push((row, capability));
    }
    Ok(rows)
}
async fn call(connection: &(Connection, String), id: Option<&str>) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(if id.is_some() { 90 } else { 5 }))
        .build()
        .map_err(|_| "Cannot connect to Codex")?;
    let url = format!(
        "http://127.0.0.1:{}/{}",
        connection.0.port,
        if id.is_some() { "select" } else { "status" }
    );
    let request = if let Some(id) = id {
        client
            .post(url)
            .header("Content-Type", "application/json")
            .body(json!({"id":id}).to_string())
    } else {
        client.get(url)
    };
    let response = request
        .bearer_auth(&connection.1)
        .send()
        .await
        .map_err(|_| "The desktop bridge is not connected. Quit and reopen Codex normally once.")?;
    let code = response.status();
    let value: Value = serde_json::from_str(
        &response
            .text()
            .await
            .map_err(|_| "Cannot read desktop confirmation")?,
    )
    .map_err(|_| "Invalid desktop confirmation")?;
    if !code.is_success() {
        return Err(value["error"]
            .as_str()
            .unwrap_or("Codex could not confirm the account switch")
            .into());
    }
    Ok(value)
}
pub async fn status() -> Result<LiveStatus, String> {
    let mut status = LiveStatus {
        enabled: configuration().is_ok_and(|c| c.enabled),
        ..Default::default()
    };
    for connection in connections().unwrap_or_default() {
        if let Ok(value) = call(&connection, None).await {
            if value["ready"] == true {
                status.connected += 1;
                status.busy |= value["busy"] == true;
                if status.runtime_account.is_none() {
                    status.runtime_account = serde_json::from_value(value["account"].clone()).ok();
                }
            }
        }
    }
    Ok(status)
}
pub async fn apply(id: &str) -> Result<usize, String> {
    let rows = connections().unwrap_or_default();
    if rows.is_empty() {
        return Err("Enable live switching, then quit and reopen Codex normally once. Meter will switch accounts without restarting Codex after the bridge connects.".into());
    }
    let saved = vault::list(&meter_root()?.join("saved-logins"))?;
    let mut previous = vec![];
    for row in &rows {
        let state = call(row, None).await?;
        if state["ready"] != true || state["busy"] == true {
            return Err("Finish or stop active Codex work or voice before switching accounts. Codex was left open.".into());
        }
        let key = state["account"]["accountKey"]
            .as_str()
            .ok_or("Cannot confirm the current desktop login; save it in Switch first")?;
        previous.push(
            saved
                .iter()
                .find(|p| p.account.account_key == key)
                .ok_or("Save the current desktop account before switching")?
                .id
                .clone(),
        );
    }
    let mut applied = 0;
    for (index, row) in rows.iter().enumerate() {
        if let Err(error) = call(row, Some(id)).await {
            let mut restored = true;
            for rollback in (0..index).rev() {
                restored &= call(&rows[rollback], Some(&previous[rollback]))
                    .await
                    .is_ok();
            }
            return Err(if restored {
                error
            } else {
                format!("{error}. Some desktop connections could not be restored; check Switch before continuing.")
            });
        }
        applied += 1;
    }
    Ok(applied)
}
pub(crate) async fn refresh_auth(
    bytes: Vec<u8>,
    expected: &str,
    force: bool,
) -> Result<Vec<u8>, String> {
    if !force {
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid saved login")?;
        let claims = value["tokens"]["access_token"]
            .as_str()
            .and_then(|s| s.split('.').nth(1))
            .and_then(|s| {
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(s.trim_end_matches('='))
                    .ok()
            })
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        if claims
            .as_ref()
            .and_then(|c| c["exp"].as_i64())
            .is_some_and(|e| e > chrono::Utc::now().timestamp() + 120)
        {
            return Ok(bytes);
        }
    }
    let home = meter_root()?.join(format!("bridge/refresh-{}", uuid::Uuid::new_v4()));
    vault::atomic_write(&home.join("auth.json"), &bytes)?;
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut client = crate::codex_client::CodexClient::in_home(tx, home.clone());
    let result = async {
        client.start().await?;
        let fresh =
            fs::read(home.join("auth.json")).map_err(|_| "Could not read refreshed login")?;
        if vault::profile(&fresh)?.account_key != expected {
            return Err("Refreshed login belongs to a different account".into());
        }
        Ok(fresh)
    }
    .await;
    let _ = client.stop_and_wait().await;
    if home
        .canonicalize()
        .ok()
        .zip(meter_root()?.canonicalize().ok())
        .is_some_and(|(path, root)| path.starts_with(root.join("bridge")))
    {
        let _ = fs::remove_dir_all(&home);
    }
    result
}

#[derive(Clone)]
struct Auth {
    bytes: Vec<u8>,
    profile: AccountProfile,
    params: Value,
}
impl Auth {
    fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        let profile = vault::profile(&bytes)?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid login")?;
        let account_id = value["tokens"]["account_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Save this login again; its ChatGPT account ID is missing")?;
        Ok(Self {
            params: json!({"type":"chatgptAuthTokens","accessToken":value["tokens"]["access_token"],"chatgptAccountId":account_id,"chatgptPlanType":profile.plan_type}),
            bytes,
            profile,
        })
    }
}
#[derive(Default)]
struct Activity {
    ready: bool,
    initialize_id: Option<String>,
    pending: HashSet<String>,
    turns: HashSet<String>,
    voice: HashSet<String>,
    account: Option<AccountProfile>,
    auth: Option<Auth>,
}
impl Activity {
    fn busy(&self) -> bool {
        !self.pending.is_empty() || !self.turns.is_empty() || !self.voice.is_empty()
    }
    fn input(&mut self, message: &Value) {
        if message["method"] == "initialize" {
            self.initialize_id = Some(message["id"].to_string());
        }
        if message["method"] == "turn/start" || message["method"] == "thread/realtime/start" {
            self.pending.insert(message["id"].to_string());
        }
        if message["method"] == "account/login/start" || message["method"] == "account/logout" {
            self.auth = None;
            self.account = None;
        }
    }
    fn output(&mut self, message: &Value) {
        let id = message["id"].to_string();
        if self.initialize_id.as_ref() == Some(&id) && message.get("result").is_some() {
            self.ready = true;
        }
        if message.get("method").is_none() {
            self.pending.remove(&id);
            if message["result"]["turn"]["status"] == "inProgress" {
                self.turns
                    .insert(message["result"]["turn"]["id"].to_string());
            }
        }
        match message["method"].as_str() {
            Some("turn/started") => {
                self.turns
                    .insert(message["params"]["turn"]["id"].to_string());
            }
            Some("turn/completed") => {
                self.turns
                    .remove(&message["params"]["turn"]["id"].to_string());
            }
            Some("thread/realtime/started") => {
                self.voice.insert(message["params"]["threadId"].to_string());
            }
            Some("thread/realtime/closed") | Some("thread/closed") => {
                self.voice
                    .remove(&message["params"]["threadId"].to_string());
            }
            _ => {}
        }
    }
}
struct Broker {
    writer: tokio::sync::Mutex<tokio::process::ChildStdin>,
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>,
    activity: Mutex<Activity>,
    gate: RwLock<()>,
    selection: tokio::sync::Mutex<()>,
    home: PathBuf,
}
impl Broker {
    async fn send(&self, message: &Value) -> Result<(), String> {
        let mut writer = self.writer.lock().await;
        writer
            .write_all(format!("{message}\n").as_bytes())
            .await
            .map_err(|_| "Codex connection closed")?;
        writer
            .flush()
            .await
            .map_err(|_| "Codex connection closed".into())
    }
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = format!("meter-auth:{}", uuid::Uuid::new_v4());
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id.clone(), tx);
        let result = async {
            self.send(&json!({"id":id,"method":method,"params":params}))
                .await?;
            timeout(Duration::from_secs(20), rx)
                .await
                .map_err(|_| "Codex did not confirm the account in time")?
                .map_err(|_| "Codex connection closed")?
        }
        .await;
        self.pending.lock().unwrap().remove(&id);
        result
    }
    async fn confirm(&self, auth: &Auth) -> Result<(), String> {
        let token = self
            .request(
                "getAuthStatus",
                json!({"includeToken":true,"refreshToken":false}),
            )
            .await?;
        if token["authToken"] != auth.params["accessToken"] {
            return Err("The running Codex engine still uses another login".into());
        }
        let account = self
            .request("account/read", json!({"refreshToken":false}))
            .await?;
        crate::accounts::from_response(&account, Some(auth.profile.clone()))?
            .ok_or("Codex did not confirm a ChatGPT account")?;
        Ok(())
    }
    // Read the engine's token, never infer its login from the file on disk.
    // A saved full login supplies refresh credentials for recovery and renewal.
    async fn runtime_auth(&self) -> Result<Auth, String> {
        let value = self
            .request(
                "getAuthStatus",
                json!({"includeToken":true,"refreshToken":false}),
            )
            .await?;
        let token = value["authToken"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("The desktop has no ChatGPT login")?;
        let runtime = crate::accounts::from_auth(&json!({"tokens":{"id_token":token}}));
        let mut candidates = vec![];
        if let Some(active) = self.activity.lock().unwrap().auth.clone() {
            candidates.push(active.bytes);
        }
        if let Ok(bytes) = fs::read(self.home.join("auth.json")) {
            candidates.push(bytes);
        }
        let directory = meter_root()?.join("saved-logins");
        for row in vault::list(&directory)? {
            if runtime
                .as_ref()
                .is_some_and(|p| p.account_key != row.account.account_key)
            {
                continue;
            }
            if let Ok(bytes) = fs::read(directory.join(format!("{}.dpapi", row.id)))
                .and_then(|b| vault::crypt(&b, false).map_err(std::io::Error::other))
            {
                candidates.push(bytes);
            }
        }
        for bytes in candidates {
            let Ok(mut data) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            let Ok(profile) = vault::profile(&bytes) else {
                continue;
            };
            let matches = runtime
                .as_ref()
                .map(|p| p.account_key == profile.account_key)
                .unwrap_or(data["tokens"]["access_token"] == token);
            if !matches {
                continue;
            }
            data["tokens"]["access_token"] = json!(token);
            let auth = Auth::parse(serde_json::to_vec(&data).unwrap())?;
            self.confirm(&auth).await?;
            self.activity.lock().unwrap().account = Some(auth.profile.clone());
            return Ok(auth);
        }
        Err("Cannot recover the running desktop login from saved accounts. Save that account before switching.".into())
    }
    async fn select(&self, id: &str) -> Result<AccountProfile, String> {
        let _selection = self
            .selection
            .try_lock()
            .map_err(|_| "Another account operation is in progress")?;
        let _gate = self.gate.write().await;
        {
            let state = self.activity.lock().unwrap();
            if !state.ready || state.busy() {
                return Err("Finish active Codex work or voice first. Codex was left open.".into());
            }
        }
        let directory = meter_root()?.join("saved-logins");
        vault::validate_id(id)?;
        let saved = vault::list(&directory)?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or("Saved account no longer exists")?;
        let bytes = vault::crypt(
            &fs::read(directory.join(format!("{id}.dpapi")))
                .map_err(|_| "Saved login is missing")?,
            false,
        )?;
        if vault::profile(&bytes)?.account_key != saved.account.account_key {
            return Err("Saved login identity does not match".into());
        }
        let previous = self.runtime_auth().await?;
        vault::save(&directory, &previous.bytes, "")?;
        let target = Auth::parse(refresh_auth(bytes, &saved.account.account_key, false).await?)?;
        self.activity.lock().unwrap().auth = Some(target.clone());
        let result = async {
            self.request("account/login/start", target.params.clone())
                .await?;
            self.confirm(&target).await?;
            vault::save(&directory, &target.bytes, &saved.label)?;
            vault::atomic_write(&self.home.join("auth.json"), &target.bytes)?;
            Ok::<(), String>(())
        }
        .await;
        if let Err(error) = result {
            self.activity.lock().unwrap().auth = Some(previous.clone());
            if self
                .request("account/login/start", previous.params.clone())
                .await
                .is_ok()
                && self.confirm(&previous).await.is_ok()
            {
                self.activity.lock().unwrap().account = Some(previous.profile);
                return Err(format!("{error}. The previous desktop login was restored."));
            }
            self.activity.lock().unwrap().account = None;
            return Err(format!(
                "{error}. Desktop login recovery requires attention; Codex was left open."
            ));
        }
        self.activity.lock().unwrap().account = Some(target.profile.clone());
        Ok(target.profile)
    }
    async fn renew(&self, message: Value) -> Value {
        let result = async {
            // Authentication reads can themselves trigger a server refresh
            // request during login. Answer from the just-activated tokens then;
            // waiting for the selection lock would deadlock confirmation.
            let selection = self.selection.try_lock().ok();
            let active = self
                .activity
                .lock()
                .unwrap()
                .auth
                .clone()
                .ok_or("No external login")?;
            if message["params"]["previousAccountId"]
                .as_str()
                .is_some_and(|id| Some(id) != active.params["chatgptAccountId"].as_str())
            {
                return Err("Account changed during renewal".into());
            }
            if selection.is_none() {
                return Ok(active.params.clone());
            }
            let source = fs::read(self.home.join("auth.json"))
                .ok()
                .filter(|b| {
                    vault::profile(b).is_ok_and(|p| p.account_key == active.profile.account_key)
                })
                .unwrap_or(active.bytes);
            let fresh =
                Auth::parse(refresh_auth(source, &active.profile.account_key, true).await?)?;
            vault::save(&meter_root()?.join("saved-logins"), &fresh.bytes, "")?;
            if crate::accounts::local_account_at(&self.home)?
                .is_some_and(|p| p.account_key == fresh.profile.account_key)
            {
                vault::atomic_write(&self.home.join("auth.json"), &fresh.bytes)?;
            }
            let params = fresh.params.clone();
            self.activity.lock().unwrap().auth = Some(fresh);
            Ok::<Value, String>(params)
        }
        .await;
        match result {
            Ok(mut params) => {
                params.as_object_mut().unwrap().remove("type");
                json!({"id":message["id"],"result":params})
            }
            Err(_) => {
                json!({"id":message["id"],"error":{"code":-32000,"message":"Meter could not renew the selected login; reconnect it in Switch"}})
            }
        }
    }
}
#[derive(Clone)]
struct HttpState {
    broker: Arc<Broker>,
    capability: String,
}
fn authorized(headers: &HeaderMap, state: &HttpState) -> bool {
    headers.get("Authorization").and_then(|v| v.to_str().ok())
        == Some(format!("Bearer {}", state.capability).as_str())
}
fn prepare_input(message: &mut Value) {
    if message["method"] == "initialize" {
        if !message["params"].is_object() {
            message["params"] = json!({});
        }
        if !message["params"]["capabilities"].is_object() {
            message["params"]["capabilities"] = json!({});
        }
        message["params"]["capabilities"]["experimentalApi"] = json!(true);
    }
    if matches!(
        message["method"].as_str(),
        Some("thread/start" | "thread/resume" | "thread/fork")
    ) && (message["params"]["modelProvider"].is_null()
        || message["params"]["modelProvider"] == "openai")
    {
        if !message["params"].is_object() {
            message["params"] = json!({});
        }
        message["params"]["modelProvider"] = json!("meter-live");
    }
}
async fn api_status(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    if !authorized(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let gate = state.broker.gate.try_read().ok();
    let probe = {
        let activity = state.broker.activity.lock().unwrap();
        gate.is_some() && activity.ready && !activity.busy() && activity.account.is_none()
    };
    if probe {
        let _ = state.broker.runtime_auth().await;
    }
    let activity = state.broker.activity.lock().unwrap();
    Ok(Json(
        json!({"ready":activity.ready,"busy":activity.busy() || gate.is_none(),"account":activity.account}),
    ))
}
async fn api_select(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !authorized(&headers, &state) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        ));
    }
    let id = body["id"].as_str().ok_or((
        StatusCode::BAD_REQUEST,
        Json(json!({"error":"Select a saved account"})),
    ))?;
    state
        .broker
        .select(id)
        .await
        .map(|account| Json(json!({"account":account})))
        .map_err(|error| (StatusCode::CONFLICT, Json(json!({"error":error}))))
}
pub fn is_bridge() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .is_some_and(|n| n == "codex-meter-bridge")
}
pub fn run() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Err(error) = rt.block_on(proxy()) {
        eprintln!("Codex Meter bridge: {error}");
        std::process::exit(1);
    }
}
async fn proxy() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let (exe, upstream_args) = upstream()?;
    if !args.iter().any(|a| a == "app-server") {
        let status = Command::new(exe)
            .args(upstream_args)
            .args(args)
            .creation_flags(0x08000000)
            .status()
            .map_err(|_| "Cannot start the Codex engine")?;
        std::process::exit(status.code().unwrap_or(1));
    }
    // A private provider keeps native ChatGPT authentication and its default
    // backend, while disabling sockets that can retain the outgoing login.
    // New engines reserve the built-in `openai` ID, so do not override it.
    let mut child = tokio::process::Command::new(exe)
        .args(upstream_args)
        .args([
            "-c",
            "model_provider=\"meter-live\"",
            "-c",
            "model_providers.meter-live.name=\"OpenAI\"",
            "-c",
            "model_providers.meter-live.requires_openai_auth=true",
            "-c",
            "model_providers.meter-live.supports_websockets=false",
        ])
        .args(args)
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "Cannot start the Codex engine")?;
    let engine_stdout = child.stdout.take().ok_or("Codex output unavailable")?;
    let broker = Arc::new(Broker {
        writer: tokio::sync::Mutex::new(child.stdin.take().ok_or("Codex input unavailable")?),
        pending: Mutex::new(HashMap::new()),
        activity: Mutex::new(Activity::default()),
        gate: RwLock::new(()),
        selection: tokio::sync::Mutex::new(()),
        home: vault::codex_home()?
            .canonicalize()
            .map_err(|_| "Cannot locate Codex home")?,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "Cannot create the private switch connection")?;
    let capability = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    let metadata = Connection {
        pid: std::process::id(),
        port: listener.local_addr().unwrap().port(),
        home: broker.home.clone(),
        capability: base64::engine::general_purpose::STANDARD
            .encode(vault::crypt(capability.as_bytes(), true)?),
    };
    let metadata_path = meter_root()?.join(format!("bridge/connections/{}.json", metadata.pid));
    vault::atomic_write(&metadata_path, &serde_json::to_vec(&metadata).unwrap())?;
    let app = Router::new()
        .route("/status", get(api_status))
        .route("/select", post(api_select))
        .layer(DefaultBodyLimit::max(1024))
        .with_state(HttpState {
            broker: broker.clone(),
            capability,
        });
    let http = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let (input_tx, mut input_rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if input_tx.send(line).is_err() {
                break;
            }
        }
    });
    let input_broker = broker.clone();
    let input = tokio::spawn(async move {
        while let Some(line) = input_rx.recv().await {
            let Ok(mut message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            prepare_input(&mut message);
            let _gate = input_broker.gate.read().await;
            input_broker.activity.lock().unwrap().input(&message);
            if input_broker.send(&message).await.is_err() {
                break;
            }
        }
    });
    let (output_tx, mut output_rx) = mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        let mut stdout = std::io::stdout().lock();
        while let Some(line) = output_rx.blocking_recv() {
            if writeln!(stdout, "{line}")
                .and_then(|_| stdout.flush())
                .is_err()
            {
                break;
            }
        }
    });
    let mut lines = BufReader::new(engine_stdout).lines();
    loop {
        tokio::select! {
            line=lines.next_line()=>{
                let Some(line)=line.map_err(|_| "Codex output closed")? else { break; };
                if let Ok(message)=serde_json::from_str::<Value>(&line) {
                    if let Some(id)=message["id"].as_str() {
                        if id.starts_with("meter-auth:") {
                            if let Some(pending)=broker.pending.lock().unwrap().remove(id) { let response=if message.get("error").is_some() { Err("Codex rejected the authentication request".into()) } else { Ok(message["result"].clone()) }; let _=pending.send(response); }
                            continue;
                        }
                    }
                    broker.activity.lock().unwrap().output(&message);
                    if message["method"]=="account/chatgptAuthTokens/refresh" && broker.activity.lock().unwrap().auth.is_some() { let b=broker.clone(); tokio::spawn(async move { let response=b.renew(message).await; let _=b.send(&response).await; }); continue; }
                }
                let _=output_tx.send(line);
            },
            _=child.wait()=>{break;},
            _=input_rx_closed(&input)=>{break;}
        }
    }
    input.abort();
    http.abort();
    let _ = child.kill().await;
    let _ = child.wait().await;
    let _ = fs::remove_file(metadata_path);
    Ok(())
}
async fn input_rx_closed(input: &tokio::task::JoinHandle<()>) {
    while !input.is_finished() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uses_http_for_existing_openai_chats_and_preserves_other_fields() {
        let mut resume = json!({"id":1,"method":"thread/resume","params":{"threadId":"old-chat","modelProvider":"openai","unknownFutureField":[1,2]}});
        prepare_input(&mut resume);
        assert_eq!(resume["params"]["modelProvider"], "meter-live");
        assert_eq!(resume["params"]["unknownFutureField"], json!([1, 2]));
        let mut other = json!({"method":"thread/start","params":{"modelProvider":"custom"}});
        prepare_input(&mut other);
        assert_eq!(other["params"]["modelProvider"], "custom");
    }
    #[test]
    fn blocks_switching_during_turns_pending_starts_and_voice() {
        let mut state = Activity::default();
        state.input(&json!({"id":1,"method":"turn/start"}));
        assert!(state.busy());
        state.output(&json!({"method":"turn/started","params":{"turn":{"id":"turn-a"}}}));
        state.output(&json!({"id":1,"result":{"turn":{"id":"turn-a","status":"inProgress"}}}));
        assert!(state.busy());
        state.output(&json!({"method":"turn/completed","params":{"turn":{"id":"turn-a"}}}));
        assert!(!state.busy());
        state.output(&json!({"method":"thread/realtime/started","params":{"threadId":"voice-a"}}));
        assert!(state.busy());
        state.output(&json!({"method":"thread/realtime/closed","params":{"threadId":"voice-a"}}));
        assert!(!state.busy());
    }
    #[test]
    fn failed_turn_start_clears_busy_marker() {
        let mut state = Activity::default();
        state.input(&json!({"id":1,"method":"turn/start"}));
        state.output(&json!({"id":1,"error":{"code":-1}}));
        assert!(!state.busy());
    }
}
