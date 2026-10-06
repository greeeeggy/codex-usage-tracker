// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--mcp") => {
            codex_meter_lib::agent_bridge::run_mcp();
            return;
        }
        Some("--quota-hook") => {
            codex_meter_lib::agent_bridge::run_hook();
            return;
        }
        _ => {}
    }
    codex_meter_lib::run()
}
