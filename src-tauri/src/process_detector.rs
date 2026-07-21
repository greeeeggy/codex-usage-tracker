use serde::{Deserialize, Serialize};
use sysinfo::System;

/// Types of Codex clients that can be detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CodexClientType {
    Desktop,
    Cli,
    VsCodeExtension,
    CursorExtension,
    Unknown,
}

/// A detected Codex client process
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedClient {
    pub client_type: CodexClientType,
    pub pid: u32,
    pub name: String,
}

/// Detect running Codex client processes on the system
pub fn detect_codex_clients() -> Vec<DetectedClient> {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut clients = Vec::new();
    let mut seen_pids = std::collections::HashSet::new();

    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_lowercase();
        let cmd: Vec<String> = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        let cmd_str = cmd.join(" ").to_lowercase();
        let exe_path = process
            .exe()
            .map(|p| p.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let pid_val = pid.as_u32();

        // Skip if already detected (avoid duplicates)
        if seen_pids.contains(&pid_val) {
            continue;
        }

        // Detect Codex Desktop app
        if name.contains("codex") && !name.contains("codex-meter") {
            // Check if it's the Codex Desktop electron app
            if exe_path.contains("codex")
                && (exe_path.contains("electron")
                    || exe_path.contains("app") && !cmd_str.contains("app-server"))
            {
                seen_pids.insert(pid_val);
                clients.push(DetectedClient {
                    client_type: CodexClientType::Desktop,
                    pid: pid_val,
                    name: format!("Codex Desktop ({})", name),
                });
                continue;
            }

            // Check if it's a codex app-server process
            if cmd_str.contains("app-server") {
                // Determine if it belongs to VS Code, Cursor, or is standalone
                let client_type = if exe_path.contains("extensions") && exe_path.contains("openai")
                {
                    if exe_path.contains("cursor") {
                        CodexClientType::CursorExtension
                    } else {
                        CodexClientType::VsCodeExtension
                    }
                } else if cmd_str.contains("codex-meter") || cmd_str.contains("codex_meter") {
                    // Skip our own process
                    continue;
                } else {
                    // Could be Desktop's bundled app-server or standalone
                    CodexClientType::Desktop
                };

                seen_pids.insert(pid_val);
                clients.push(DetectedClient {
                    client_type,
                    pid: pid_val,
                    name: format!("codex app-server ({})", pid_val),
                });
                continue;
            }

            // Plain codex CLI process (not app-server)
            if name == "codex" || name == "codex.exe" || name == "codex.cmd" {
                if !cmd_str.contains("app-server") && !cmd_str.contains("--version") {
                    seen_pids.insert(pid_val);
                    clients.push(DetectedClient {
                        client_type: CodexClientType::Cli,
                        pid: pid_val,
                        name: format!("Codex CLI ({})", pid_val),
                    });
                }
            }
        }

        // Detect VS Code with Codex extension running
        // The extension spawns a separate codex app-server child process,
        // which is caught above. We could also detect Code.exe but that
        // doesn't necessarily mean Codex is active.
    }

    clients
}

/// Check if any Codex client is currently running
#[allow(dead_code)]
pub fn is_codex_running() -> bool {
    !detect_codex_clients().is_empty()
}
