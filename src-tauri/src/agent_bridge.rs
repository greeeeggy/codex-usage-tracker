//! Read-only stdio MCP and official lifecycle hook integration. No credentials on this interface.
use base64::Engine;
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
const GUIDANCE: &str = "Codex Meter monitors allowance automatically through its installed hooks. Do not call get_usage_guard routinely or between work steps. The hooks deliver one quota alert per chat and reset cycle at 5% or less remaining. Follow that alert to preserve a handoff and schedule one continuation of this same chat. Use get_usage_guard only for an explicit usage question or to verify fresh quota on that scheduled continuation. Never claim a schedule exists until its scheduling tool succeeds.";
pub fn rpc_result(request: &Value, guard: Result<Value, String>) -> Option<Value> {
    let id = request.get("id")?.clone();
    let method = request["method"].as_str().unwrap_or("");
    let result = match method {
        "initialize" => {
            json!({"protocolVersion":request["params"]["protocolVersion"].as_str().unwrap_or("2024-11-05"), "capabilities":{"tools":{}}, "serverInfo":{"name":"codex-meter", "version":env!("CARGO_PKG_VERSION")}, "instructions":GUIDANCE})
        }
        "ping" => json!({}),
        "tools/list" => {
            json!({"tools":[{"name":"get_usage_guard", "description":"Read shared Codex allowance when the user asks or once on a scheduled quota continuation. Installed hooks monitor automatically; do not poll this tool during ordinary work. No account switching or credentials exposed.", "inputSchema":{"type":"object","properties":{},"additionalProperties":false}, "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}}, {"name":"quota_checkpoint", "description":"Compatibility hook for older Meter connections. Supplies one quota alert per chat/reset; do not call this tool manually.", "inputSchema":{"type":"object","properties":{"hook_event_name":{"type":"string"},"session_id":{"type":"string"},"turn_id":{"type":"string"},"stop_hook_active":{"type":"boolean"}},"required":["hook_event_name","session_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}}]})
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
    let prompt = guard["prompt"].as_str().unwrap_or("");
    let low = guard["shouldPause"] == true && !prompt.is_empty();
    if event == "Stop" {
        if low && !already_delivered && input["stop_hook_active"] != true {
            return json!({"decision":"block","reason":prompt});
        }
        return json!({});
    }
    if low && !already_delivered {
        return json!({"hookSpecificOutput":{"hookEventName":event,"additionalContext":prompt}});
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
    let Ok(home) = crate::account_switch::codex_home() else {
        return json!({});
    };
    checkpoint_result_at(input, guard, &home)
}
fn checkpoint_result_at(input: &Value, guard: &Value, home: &std::path::Path) -> Value {
    let result = hook_result(input, guard, false);
    if result == json!({})
        || input["session_id"].as_str().is_none_or(str::is_empty)
        || guard["checkpointId"].as_str().is_none_or(str::is_empty)
    {
        return json!({});
    }
    // Claim once per chat/reset, including concurrent hooks and later turns.
    use sha2::{Digest, Sha256};
    let key = format!(
        "{:x}",
        Sha256::digest(format!("{}:{}", input["session_id"], guard["checkpointId"]))
    );
    let directory = home.join("meter-guard-checkpoints");
    if fs::create_dir_all(&directory).is_err()
        || fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(key))
            .is_err()
    {
        return json!({});
    }
    result
}
fn merge_hooks(mut document: Value, enabled: bool, exe: &std::path::Path) -> Result<Value, String> {
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
    let script = format!(
        "& '{}' --quota-hook",
        exe.display().to_string().replace('\'', "''")
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    for event in ["SessionStart", "UserPromptSubmit", "PreToolUse", "Stop"] {
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
            groups.push(json!({"matcher":if event=="PreToolUse" { ".*" } else { "" },"hooks":[{"type":"command","command":format!("\"{}\" --quota-hook",exe.display()),"commandWindows":format!("powershell.exe -NoProfile -NonInteractive -EncodedCommand {encoded}"),"timeout":5}]}));
        }
    }
    Ok(document)
}
fn is_meter_hook(handler: &Value) -> bool {
    (handler["server"] == "codex-meter" && handler["tool"] == "quota_checkpoint")
        || handler["command"].as_str().is_some_and(|c| {
            c.ends_with(" --quota-hook")
                && (handler["statusMessage"] == "Checking Codex allowance"
                    || c.to_ascii_lowercase().contains("codex-meter"))
        })
}

pub(crate) fn repair_guard_integration() -> Result<(), String> {
    let path = crate::account_switch::codex_home()?.join("hooks.json");
    let original = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Cannot read existing hooks".into()),
    };
    let document: Value = serde_json::from_slice(&original)
        .map_err(|_| "Existing hooks.json is invalid; it was left unchanged")?;
    let installed = document["hooks"].as_object().is_some_and(|events| {
        events
            .values()
            .filter_map(Value::as_array)
            .flatten()
            .any(|g| {
                g["hooks"]
                    .as_array()
                    .is_some_and(|handlers| handlers.iter().any(is_meter_hook))
            })
    });
    if installed {
        let exe = std::env::current_exe().map_err(|_| "Cannot locate Meter executable")?;
        let next = merge_hooks(document.clone(), true, &exe)?;
        if next != document {
            let backup = path.with_extension("json.before-meter-automatic.bak");
            if !backup.exists() {
                fs::copy(&path, backup).map_err(|_| "Cannot back up existing hooks")?;
            }
            if fs::read(&path).ok().as_deref() != Some(original.as_slice()) {
                return Err("Codex hooks changed during setup; try again".into());
            }
            crate::account_switch::atomic_write(&path, &serde_json::to_vec_pretty(&next).unwrap())?;
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn get_usage_guard(
    state: tauri::State<'_, std::sync::Arc<tokio::sync::RwLock<crate::usage_service::UsageState>>>,
) -> Result<Value, String> {
    Ok(crate::quota_guard::evaluate(
        &*state.read().await,
        chrono::Utc::now(),
    ))
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
    let next = merge_hooks(document, enabled, &exe)?;
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
        let exe = std::path::Path::new("C:/Meter's folder/Codex-Meter.exe");
        let original = json!({"description":"mine","hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"other.exe"},{"type":"mcp_tool","server":"codex-meter","tool":"quota_checkpoint"}]}]}});
        let once = merge_hooks(original, true, exe).unwrap();
        let twice = merge_hooks(once.clone(), true, exe).unwrap();
        assert_eq!(once, twice);
        for event in ["SessionStart", "UserPromptSubmit", "PreToolUse", "Stop"] {
            let handler = &once["hooks"][event].as_array().unwrap().last().unwrap()["hooks"][0];
            assert_eq!(handler["type"], "command");
            assert!(handler.get("input").is_none());
            assert!(handler.get("statusMessage").is_none());
            let command = handler["commandWindows"].as_str().unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(command.split_whitespace().last().unwrap())
                .unwrap();
            let script = String::from_utf16(
                &bytes
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            assert_eq!(
                script,
                "& 'C:/Meter''s folder/Codex-Meter.exe' --quota-hook"
            );
        }
        let removed = merge_hooks(twice, false, exe).unwrap();
        assert_eq!(removed["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
        assert_eq!(removed["description"], "mine");
    }
    #[test]
    fn quota_alerts_are_silent_until_low_and_once_across_turns_and_concurrent_hooks() {
        let home = std::env::temp_dir().join(format!("meter-alert-{}", uuid::Uuid::new_v4()));
        let input = json!({"hook_event_name":"PreToolUse", "session_id":"chat", "turn_id":"one"});
        for event in ["SessionStart", "UserPromptSubmit", "PreToolUse", "Stop"] {
            assert_eq!(
                hook_result(
                    &json!({"hook_event_name":event}),
                    &json!({"shouldPause":false}),
                    false
                ),
                json!({})
            );
        }
        assert!(!home.exists());
        let guard =
            json!({"shouldPause":true,"checkpointId":"account:reset","prompt":"Save and schedule"});
        let delivered = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| checkpoint_result_at(&input, &guard, &home)))
                .collect();
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .filter(|result| *result != json!({}))
                .count()
        });
        assert_eq!(delivered, 1);
        let mut later = input.clone();
        later["turn_id"] = json!("two");
        assert_eq!(checkpoint_result_at(&later, &guard, &home), json!({}));
        let mut next = guard.clone();
        next["checkpointId"] = json!("account:next-reset");
        assert_ne!(checkpoint_result_at(&input, &next, &home), json!({}));
        assert!(
            rpc_result(&json!({"id":1,"method":"initialize"}), Ok(Value::Null)).unwrap()["result"]
                ["instructions"]
                .as_str()
                .unwrap()
                .contains("Do not call get_usage_guard routinely")
        );
        fs::remove_dir_all(home).unwrap();
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
