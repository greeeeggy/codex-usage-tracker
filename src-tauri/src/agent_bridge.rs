//! Read-only stdio MCP and official lifecycle hook integration. No credentials on this interface.
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, Write},
    os::windows::process::CommandExt,
    process::Command,
};

pub async fn fetch_guard() -> Result<Value, String> {
    let response = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|_| "Cannot create local client")?
        .get("http://127.0.0.1:32145/api/guard")
        .send()
        .await
        .map_err(|_| "Codex Meter is unavailable. Start Meter and try again.")?
        .error_for_status()
        .map_err(|_| "Usage guard is unavailable")?;
    serde_json::from_str(
        &response
            .text()
            .await
            .map_err(|_| "Cannot read usage guard")?,
    )
    .map_err(|_| "Invalid usage guard response".into())
}
const GUIDANCE: &str = "Use Codex Meter get_usage_guard at the start of work and between substantial steps. At 5% or less remaining, preserve a handoff, schedule one continuation of this same chat at the returned resumeAt timestamp, and end the turn. On continuation, recheck fresh quota before resuming. Never claim a schedule exists until its scheduling tool succeeds.";
pub fn rpc_result(request: &Value, guard: Result<Value, String>) -> Option<Value> {
    let id = request.get("id")?.clone();
    let method = request["method"].as_str().unwrap_or("");
    let result = match method {
        "initialize" => {
            json!({"protocolVersion":request["params"]["protocolVersion"].as_str().unwrap_or("2024-11-05"), "capabilities":{"tools":{}}, "serverInfo":{"name":"codex-meter", "version":env!("CARGO_PKG_VERSION")}, "instructions":GUIDANCE})
        }
        "ping" => json!({}),
        "tools/list" => {
            json!({"tools":[{"name":"get_usage_guard", "description":"Read fresh shared Codex allowance and the exact pause/handoff/resume instructions. Call before work and at checkpoints. No account switching or credentials exposed.", "inputSchema":{"type":"object","properties":{},"additionalProperties":false}, "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}}, {"name":"quota_checkpoint", "description":"Codex lifecycle hook: supply quota handoff context once per chat/turn/reset. Used by Meter's installed hooks; does not schedule or change accounts.", "inputSchema":{"type":"object","properties":{"hook_event_name":{"type":"string"},"session_id":{"type":"string"},"turn_id":{"type":"string"},"stop_hook_active":{"type":"boolean"}},"required":["hook_event_name","session_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}}]})
        }
        "tools/call" if request["params"]["name"] == "get_usage_guard" => match guard {
            Ok(g) => json!({"content":[{"type":"text","text":g.to_string()}],"isError":false}),
            Err(e) => json!({"content":[{"type":"text","text":e}],"isError":true}),
        },
        "tools/call" if request["params"]["name"] == "quota_checkpoint" => {
            let guard = guard.unwrap_or_else(|_| json!({"shouldPause":false}));
            let result = checkpoint_result(&request["params"]["arguments"], &guard);
            json!({"content":[{"type":"text","text":result.to_string()}],"isError":false})
        }
        _ => {
            return Some(
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unknown method or tool"}}),
            )
        }
    };
    Some(json!({"jsonrpc":"2.0", "id":id, "result":result}))
}
pub fn run_mcp() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            let _ = writeln!(
                stdout,
                "{}",
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid JSON"}})
            );
            let _ = stdout.flush();
            continue;
        };
        let guard = if request["method"] == "tools/call" {
            rt.block_on(fetch_guard())
        } else {
            Ok(Value::Null)
        };
        if let Some(result) = rpc_result(&request, guard) {
            let _ = writeln!(stdout, "{result}");
            let _ = stdout.flush();
        }
    }
}
pub fn hook_result(input: &Value, guard: &Value, already_delivered: bool) -> Value {
    let event = input["hook_event_name"].as_str().unwrap_or("PreToolUse");
    let low = guard["shouldPause"] == true;
    let prompt = guard["prompt"].as_str().unwrap_or("");
    if event == "Stop" {
        if low && !already_delivered && input["stop_hook_active"] != true {
            return json!({"decision":"block","reason":prompt});
        }
        return json!({});
    }
    if low && !already_delivered {
        return json!({"hookSpecificOutput":{"hookEventName":event,"additionalContext":prompt}});
    }
    if event == "SessionStart" {
        return json!({"hookSpecificOutput":{"hookEventName":event,"additionalContext":GUIDANCE}});
    }
    json!({})
}
pub fn run_hook() {
    let mut text = String::new();
    use std::io::Read;
    let _ = std::io::stdin().read_to_string(&mut text);
    let input: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let guard = rt
        .block_on(fetch_guard())
        .unwrap_or_else(|_| json!({"shouldPause":false}));
    println!("{}", checkpoint_result(&input, &guard));
}
fn checkpoint_result(input: &Value, guard: &Value) -> Value {
    // One warning per chat/turn/reset; persistent across separate hook processes.
    use sha2::{Digest, Sha256};
    let key = format!(
        "{:x}",
        Sha256::digest(format!(
            "{}:{}:{}",
            input["session_id"], input["turn_id"], guard["checkpointId"]
        ))
    );
    let directory = crate::account_switch::codex_home()
        .ok()
        .map(|p| p.join("meter-guard-checkpoints"));
    let marker = directory.map(|p| p.join(key));
    let already = marker.as_ref().is_some_and(|p| p.exists());
    let result = hook_result(&input, &guard, already);
    if result != json!({}) && guard["shouldPause"] == true {
        if let Some(path) = marker {
            let _ = crate::account_switch::atomic_write(&path, b"checkpoint delivered");
        }
    }
    result
}
fn merge_hooks(mut document: Value, enabled: bool) -> Result<Value, String> {
    if !document.is_object() {
        return Err("Existing hooks.json is not an object; it was left unchanged".into());
    }
    let hooks = document
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("Existing hooks configuration is invalid")?;
    for event in ["SessionStart", "PreToolUse", "Stop"] {
        let groups = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Existing hook event is invalid")?;
        // Only remove our own marked groups; preserve every unrelated handler.
        for group in groups.iter_mut() {
            if let Some(handlers) = group["hooks"].as_array_mut() {
                handlers.retain(|h| !is_meter_hook(h));
            }
        }
        groups.retain(|g| g["hooks"].as_array().is_none_or(|h| !h.is_empty()));
        if enabled {
            let mut input = json!({"hook_event_name":event,"session_id":"${session_id}"});
            if event != "SessionStart" {
                input["turn_id"] = json!("${turn_id}");
            }
            if event == "Stop" {
                input["stop_hook_active"] = json!("${stop_hook_active}");
            }
            groups.push(json!({"matcher":if event=="PreToolUse" { ".*" } else { "" },"hooks":[{"type":"mcp_tool","server":"codex-meter","tool":"quota_checkpoint","input":input,"timeout":5,"statusMessage":"Checking Codex allowance"}]}));
        }
    }
    Ok(document)
}
fn is_meter_hook(handler: &Value) -> bool {
    (handler["server"] == "codex-meter" && handler["tool"] == "quota_checkpoint")
        || (handler["statusMessage"] == "Checking Codex allowance"
            && handler["command"]
                .as_str()
                .is_some_and(|c| c.ends_with(" --quota-hook")))
}
#[tauri::command]
pub async fn get_usage_guard(
    state: tauri::State<'_, std::sync::Arc<tokio::sync::RwLock<crate::usage_service::UsageState>>>,
) -> Value {
    crate::quota_guard::evaluate(&*state.read().await, chrono::Utc::now())
}
#[tauri::command]
pub fn get_guard_integration() -> Result<Value, String> {
    let path = crate::account_switch::codex_home()?.join("hooks.json");
    let installed = fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| {
            v["hooks"]["PreToolUse"].as_array().is_some_and(|groups| {
                groups.iter().any(|g| {
                    g["hooks"]
                        .as_array()
                        .is_some_and(|handlers| handlers.iter().any(|h| is_meter_hook(h)))
                })
            })
        });
    Ok(json!({"installed":installed,"endpoint":"http://127.0.0.1:32145/api/guard"}))
}
#[tauri::command]
pub async fn configure_usage_guard(
    manager: tauri::State<'_, crate::account_switch::SwitchManager>,
    enabled: bool,
) -> Result<Value, String> {
    let _guard = manager
        .operation
        .try_lock()
        .map_err(|_| "An account operation is already in progress")?;
    let exe = std::env::current_exe().map_err(|_| "Cannot locate Meter executable")?;
    let home = crate::account_switch::codex_home()?;
    let path = home.join("hooks.json");
    let document = match fs::read(&path) {
        Ok(b) => serde_json::from_slice(&b)
            .map_err(|_| "Existing hooks.json is invalid; it was left unchanged")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(_) => return Err("Cannot read existing hooks".into()),
    };
    let next = merge_hooks(document, enabled)?;
    let cli = crate::codex_client::find_codex_executable()?;
    let output = tauri::async_runtime::spawn_blocking(move || {
        let mut command = Command::new(cli);
        command.env("CODEX_HOME", &home).creation_flags(0x08000000);
        if enabled {
            command
                .args(["mcp", "add", "codex-meter", "--"])
                .arg(&exe)
                .arg("--mcp");
        } else {
            command.args(["mcp", "remove", "codex-meter"]);
        }
        command.output()
    })
    .await
    .map_err(|_| "Could not configure Codex connection")?
    .map_err(|_| "Could not configure Codex connection")?;
    if !output.status.success() {
        return Err(
            "Codex could not register the Meter connection. Check the installed CLI version."
                .into(),
        );
    }
    if path.exists() {
        fs::copy(&path, path.with_extension("json.meter-backup"))
            .map_err(|_| "Cannot back up existing hooks")?;
    }
    crate::account_switch::atomic_write(&path, &serde_json::to_vec_pretty(&next).unwrap())?;
    get_guard_integration()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mcp_never_offers_switch_or_secret_tools() {
        let result = rpc_result(&json!({"id":1,"method":"tools/list"}), Ok(Value::Null)).unwrap();
        assert_eq!(result["result"]["tools"].as_array().unwrap().len(), 2);
        assert_eq!(result["result"]["tools"][0]["name"], "get_usage_guard");
        assert!(rpc_result(
            &json!({"method":"notifications/initialized"}),
            Ok(Value::Null)
        )
        .is_none());
    }
    #[test]
    fn preserves_unrelated_hooks_and_install_is_idempotent() {
        let original = json!({"description":"mine","hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"other.exe"}]}]}});
        let once = merge_hooks(original, true).unwrap();
        let twice = merge_hooks(once.clone(), true).unwrap();
        assert_eq!(once, twice);
        let removed = merge_hooks(twice, false).unwrap();
        assert_eq!(removed["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
        assert_eq!(removed["description"], "mine");
    }
    #[test]
    fn stop_hook_never_loops_and_checkpoint_is_context_not_tool_denial() {
        let g = json!({"shouldPause":true,"prompt":"Save and schedule"});
        let pre = hook_result(&json!({"hook_event_name":"PreToolUse"}), &g, false);
        assert_eq!(
            pre["hookSpecificOutput"]["additionalContext"],
            "Save and schedule"
        );
        assert!(pre["hookSpecificOutput"]["permissionDecision"].is_null());
        assert_eq!(
            hook_result(
                &json!({"hook_event_name":"Stop","stop_hook_active":true}),
                &g,
                false
            ),
            json!({})
        );
        assert_eq!(
            hook_result(&json!({"hook_event_name":"Stop"}), &g, false)["decision"],
            "block"
        );
    }
}
