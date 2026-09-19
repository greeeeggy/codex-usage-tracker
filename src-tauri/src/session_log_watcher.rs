use crate::db::Db;
use crate::usage_service::UsageState;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

#[derive(Deserialize, Debug)]
struct RolloutLine {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    event_type: String,
    payload: Option<RolloutPayload>,
}

#[derive(Deserialize, Debug)]
struct RolloutPayload {
    #[serde(rename = "type")]
    payload_type: String,
    info: Option<RolloutInfo>,
}

#[derive(Deserialize, Debug)]
struct RolloutInfo {
    total_token_usage: Option<RawTokenUsage>,
    last_token_usage: Option<RawTokenUsage>,
    model_context_window: Option<i64>,
}

#[derive(Deserialize, Debug)]
struct RawTokenUsage {
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    reasoning_output_tokens: Option<i64>,
    total_tokens: Option<i64>,
}

struct RolloutInfoWithTimestamp {
    timestamp: Option<String>,
    info: RolloutInfo,
}

struct TrackedFile {
    last_modified: std::time::SystemTime,
    last_read_offset: u64,
}

fn get_sessions_dir() -> Option<PathBuf> {
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        Some(PathBuf::from(user_profile).join(".codex").join("sessions"))
    } else if let Ok(home) = std::env::var("HOME") {
        Some(PathBuf::from(home).join(".codex").join("sessions"))
    } else {
        None
    }
}

fn find_rollout_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_rollout_files(&path, files);
            } else if path.is_file() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                        files.push(path);
                    }
                }
            }
        }
    }
}

fn parse_rollout_line(line: &str) -> Option<RolloutInfoWithTimestamp> {
    let raw: RolloutLine = serde_json::from_str(line).ok()?;
    if raw.event_type == "event_msg" {
        if let Some(payload) = raw.payload {
            if payload.payload_type == "token_count" {
                if let Some(info) = payload.info {
                    return Some(RolloutInfoWithTimestamp {
                        timestamp: raw.timestamp,
                        info,
                    });
                }
            }
        }
    }
    None
}

/// The background task running loop
pub async fn run(db: Arc<Db>, app_handle: tauri::AppHandle, state: Arc<RwLock<UsageState>>) {
    log::info!("[watcher] Starting Codex session log watcher background task...");

    let sessions_dir = match get_sessions_dir() {
        Some(dir) => dir,
        None => {
            log::error!(
                "[watcher] Could not determine user profile/home directory. Log watcher disabled."
            );
            return;
        }
    };

    log::info!(
        "[watcher] Monitoring sessions directory: {:?}",
        sessions_dir
    );

    let mut tracked_files: HashMap<PathBuf, TrackedFile> = HashMap::new();
    let mut initial_scan = true;

    loop {
        if !sessions_dir.exists() {
            sleep(Duration::from_secs(5)).await;
            continue;
        }

        let mut current_files = Vec::new();
        find_rollout_files(&sessions_dir, &mut current_files);

        for path in current_files {
            let metadata = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let new_size = metadata.len();
            let new_mod_time = metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            if !tracked_files.contains_key(&path) {
                let offset = if initial_scan {
                    // If modified in the last 15 minutes, read it from the beginning to catch recent actions.
                    // Otherwise, start tailing from the end.
                    let elapsed = new_mod_time.elapsed().unwrap_or(Duration::from_secs(99999));
                    if elapsed < Duration::from_secs(900) {
                        log::info!(
                            "[watcher] Tracking recent session from start: {:?}",
                            path.file_name().unwrap_or_default()
                        );
                        0
                    } else {
                        new_size
                    }
                } else {
                    // New file detected after startup, read from beginning.
                    log::info!(
                        "[watcher] Tracking new session: {:?}",
                        path.file_name().unwrap_or_default()
                    );
                    0
                };

                tracked_files.insert(
                    path.clone(),
                    TrackedFile {
                        last_modified: new_mod_time,
                        last_read_offset: offset,
                    },
                );

                // If starting from 0, fall through to read the file immediately
                if offset == new_size {
                    continue;
                }
            }

            let tracked = tracked_files.get_mut(&path).unwrap();

            if new_size > tracked.last_read_offset || new_mod_time > tracked.last_modified {
                let file = match File::open(&path) {
                    Ok(f) => f,
                    Err(_) => continue,
                };

                let mut buf_reader = BufReader::new(file);
                if buf_reader
                    .seek(SeekFrom::Start(tracked.last_read_offset))
                    .is_err()
                {
                    continue;
                }

                let session_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown_session");

                let mut lines = Vec::new();
                for line in buf_reader.lines().map_while(Result::ok) {
                    if !line.trim().is_empty() {
                        lines.push(line);
                    }
                }

                // Update tracking info
                tracked.last_read_offset = new_size;
                tracked.last_modified = new_mod_time;

                let events: Vec<_> = lines
                    .iter()
                    .filter_map(|line| parse_rollout_line(line))
                    .collect();
                let latest_index = events.len().checked_sub(1);

                for (index, event) in events.iter().enumerate() {
                    process_token_count_event(
                        &db,
                        &app_handle,
                        &state,
                        session_id,
                        event.timestamp.as_deref(),
                        &event.info,
                        Some(index) == latest_index,
                    )
                    .await;
                }
            }
        }

        initial_scan = false;
        sleep(Duration::from_secs(1)).await;
    }
}

async fn process_token_count_event(
    db: &Db,
    app_handle: &tauri::AppHandle,
    state: &RwLock<UsageState>,
    session_id: &str,
    timestamp: Option<&str>,
    info: &RolloutInfo,
    update_latest_context: bool,
) {
    let total_usage = match &info.total_token_usage {
        Some(t) => t,
        None => return,
    };
    let last_usage = match &info.last_token_usage {
        Some(l) => l,
        None => return,
    };

    let new_total = total_usage.total_tokens.unwrap_or(0);
    if new_total <= 0 {
        return;
    }

    // Query database for current cumulative totals for this session
    let current_cumulative = match db.get_session_cumulative_totals(session_id) {
        Ok(c) => c,
        Err(e) => {
            log::error!("[watcher] Failed to query cumulative totals: {}", e);
            return;
        }
    };

    let delta_total = new_total - current_cumulative.total_tokens;
    if delta_total > 0 {
        let new_input = total_usage.input_tokens.unwrap_or(0);
        let new_cached = total_usage.cached_input_tokens.unwrap_or(0);
        let new_output = total_usage.output_tokens.unwrap_or(0);
        let new_reasoning = total_usage.reasoning_output_tokens.unwrap_or(0);

        let delta_input = (new_input - current_cumulative.input_tokens).max(0);
        let delta_cached = (new_cached - current_cumulative.cached_input_tokens).max(0);
        let delta_output = (new_output - current_cumulative.output_tokens).max(0);
        let delta_reasoning =
            (new_reasoning - current_cumulative.reasoning_tokens.unwrap_or(0)).max(0);
        let captured_at = timestamp
            .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
            .map(|dt| dt.timestamp())
            .unwrap_or_else(|| chrono::Utc::now().timestamp());

        let event = crate::db::TokenEvent {
            id: format!("{}:{}", session_id, new_total),
            account_key: "default".to_string(),
            captured_at,
            client_type: "local_session".to_string(),
            thread_id: Some(session_id.to_string()),
            turn_id: None,
            project_path_hash: None,
            model: None,
            input_tokens: delta_input,
            cached_input_tokens: delta_cached,
            output_tokens: delta_output,
            reasoning_tokens: Some(delta_reasoning),
            total_tokens: delta_total,
            event_type: "local_rollout".to_string(),
        };

        if let Err(e) = db.insert_token_event(&event) {
            log::error!("[watcher] Failed to insert rollout token event: {}", e);
            return;
        }

        log::info!(
            "[watcher] Recorded token count for session {}: +{} tokens (input: {}, output: {}, reasoning: {})",
            session_id, delta_total, delta_input, delta_output, delta_reasoning
        );
    } else if !update_latest_context {
        return;
    }

    // A file can contain many historical token_count lines. Only its latest line
    // should update live context state and trigger an aggregate recalculation.
    if !update_latest_context {
        return;
    }

    // Calculate context load percentage
    let context_window = info.model_context_window.unwrap_or(0);
    let context_load_percent = if context_window > 0 {
        let last_in = last_usage.input_tokens.unwrap_or(0);
        Some(((last_in as f64) / (context_window as f64) * 100.0).min(100.0))
    } else {
        None
    };

    let last_request_tokens = crate::db::TokenBreakdown {
        input_tokens: last_usage.input_tokens.unwrap_or(0),
        cached_input_tokens: last_usage.cached_input_tokens.unwrap_or(0),
        uncached_input_tokens: (last_usage.input_tokens.unwrap_or(0)
            - last_usage.cached_input_tokens.unwrap_or(0))
        .max(0),
        output_tokens: last_usage.output_tokens.unwrap_or(0),
        reasoning_tokens: Some(last_usage.reasoning_output_tokens.unwrap_or(0)),
        total_tokens: last_usage.total_tokens.unwrap_or(
            last_usage.input_tokens.unwrap_or(0) + last_usage.output_tokens.unwrap_or(0),
        ),
    };

    let snapshot_to_emit = {
        let mut s = state.write().await;
        if let Some(ref mut snapshot) = s.snapshot {
            snapshot.latest_context_window = if context_window > 0 {
                Some(context_window)
            } else {
                None
            };
            snapshot.latest_context_load_percent = context_load_percent;
            snapshot.latest_last_request_tokens = Some(last_request_tokens);
            snapshot.clone()
        } else {
            let snapshot = crate::codex_client::UsageSnapshot {
                captured_at: chrono::Utc::now().to_rfc3339(),
                limit_id: None,
                limit_name: None,
                plan_type: None,
                rate_limit_reached_type: None,
                credits: None,
                windows: Vec::new(),
                latest_context_window: if context_window > 0 {
                    Some(context_window)
                } else {
                    None
                },
                latest_context_load_percent: context_load_percent,
                latest_last_request_tokens: Some(last_request_tokens),
            };
            s.snapshot = Some(snapshot.clone());
            snapshot
        }
    };

    let _ = app_handle.emit("usage-updated", &snapshot_to_emit);
    if let Err(e) = crate::usage_service::refresh_token_totals(db, state, app_handle).await {
        log::error!("[watcher] Failed to calculate token totals: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rollout_token_count_lines() {
        let line = r#"{"timestamp":"2026-08-30T11:30:00.041Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":34409,"cached_input_tokens":0,"output_tokens":222,"reasoning_output_tokens":138,"total_tokens":34631},"last_token_usage":{"input_tokens":34409,"cached_input_tokens":0,"output_tokens":222,"reasoning_output_tokens":138,"total_tokens":34631},"model_context_window":258400}}}"#;

        let event = parse_rollout_line(line).expect("token_count line should parse");
        assert_eq!(event.timestamp.as_deref(), Some("2026-08-30T11:30:00.041Z"));
        assert_eq!(event.info.model_context_window, Some(258_400));
        assert_eq!(
            event
                .info
                .total_token_usage
                .and_then(|usage| usage.total_tokens),
            Some(34_631)
        );
    }
}
