//! Durable local usage; app-server notifications are not summed twice.
use crate::db::{Db, TokenBreakdown, TokenEvent};
use crate::session_history::DetailedTokenUsage;
use crate::usage_service::UsageState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Checkpoint {
    previous: Option<DetailedTokenUsage>,
    model: Option<String>,
}

struct LiveContext {
    timestamp: i64,
    context_window: Option<i64>,
    last: TokenBreakdown,
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files(&path, out);
            } else if path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
            {
                out.push(path);
            }
        }
    }
}

fn delta(
    previous: Option<&DetailedTokenUsage>,
    total: &DetailedTokenUsage,
    last: &DetailedTokenUsage,
) -> Option<DetailedTokenUsage> {
    // Forks inherit cumulative usage. Context resets may decrease counters.
    let Some(previous) = previous.filter(|p| total.total_tokens >= p.total_tokens) else {
        return (last.total_tokens > 0).then_some(last.clone());
    };
    if previous.total_tokens == total.total_tokens {
        return None;
    }
    let input = (total.input_tokens - previous.input_tokens).max(0);
    let cached = (total.cached_input_tokens - previous.cached_input_tokens).max(0);
    let writes = (total.cache_write_input_tokens - previous.cache_write_input_tokens).max(0);
    Some(DetailedTokenUsage {
        input_tokens: input,
        cached_input_tokens: cached,
        cache_write_input_tokens: writes,
        uncached_input_tokens: (input - cached - writes).max(0),
        output_tokens: (total.output_tokens - previous.output_tokens).max(0),
        reasoning_tokens: (total.reasoning_tokens - previous.reasoning_tokens).max(0),
        total_tokens: total.total_tokens - previous.total_tokens,
    })
}

fn import_batch(db: &Db, path: &Path) -> Result<Option<LiveContext>, String> {
    let key = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid rollout filename")?;
    let saved = db.get_rollout_checkpoint(key)?;
    let (mut offset, mut checkpoint) = saved
        .and_then(|(offset, json)| {
            serde_json::from_str::<Checkpoint>(&json)
                .ok()
                .map(|state| (offset, state))
        })
        .unwrap_or_default();
    let file = File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if offset == len {
        return Ok(None);
    }
    if offset > len {
        offset = 0;
        checkpoint = Checkpoint::default();
    }
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut pending = Vec::new();
    let mut latest = None;
    // Bound each pass so old files cannot starve active files.
    for _ in 0..2000 {
        let mut line = String::new();
        let count = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if count == 0 || !line.ends_with('\n') {
            break;
        }
        let row_offset = offset;
        offset += count as u64;
        if !["token_count", "turn_context"]
            .iter()
            .any(|s| line.contains(s))
        {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let payload = &row["payload"];
        if row["type"] == "turn_context" {
            checkpoint.model = payload["model"].as_str().map(str::to_owned);
            continue;
        }
        if row["type"] != "event_msg" || payload["type"] != "token_count" {
            continue;
        }
        let Some(timestamp) = row["timestamp"].as_str() else {
            continue;
        };
        let Ok(time) = chrono::DateTime::parse_from_rfc3339(timestamp) else {
            continue;
        };
        let captured = time.timestamp();
        if let Some(snapshot) =
            crate::codex_client::snapshot_from_rollout(&payload["rate_limits"], timestamp)
        {
            crate::limit_history::persist_snapshot(db, &snapshot);
        }
        let Some(total) = payload
            .pointer("/info/total_token_usage")
            .filter(|v| v.is_object())
        else {
            continue;
        };
        let Some(last) = payload
            .pointer("/info/last_token_usage")
            .filter(|v| v.is_object())
        else {
            continue;
        };
        let total = DetailedTokenUsage::from_value(total);
        let last = DetailedTokenUsage::from_value(last);
        if let Some(usage) = delta(checkpoint.previous.as_ref(), &total, &last) {
            let limit_id = payload
                .pointer("/rate_limits/limit_id")
                .and_then(Value::as_str)
                .unwrap_or("unattributed")
                .to_string();
            pending.push((
                TokenEvent {
                    id: format!("{key}:{row_offset}"),
                    account_key: "default".into(),
                    captured_at: captured,
                    client_type: "local_session".into(),
                    thread_id: Some(key.into()),
                    turn_id: None,
                    project_path_hash: None,
                    model: checkpoint.model.clone(),
                    input_tokens: usage.input_tokens,
                    cached_input_tokens: usage.cached_input_tokens,
                    output_tokens: usage.output_tokens,
                    reasoning_tokens: Some(usage.reasoning_tokens),
                    total_tokens: usage.total_tokens,
                    event_type: "local_rollout".into(),
                },
                limit_id,
            ));
        }
        checkpoint.previous = Some(total);
        latest = Some(LiveContext {
            timestamp: captured,
            context_window: payload
                .pointer("/info/model_context_window")
                .and_then(Value::as_i64),
            last: TokenBreakdown {
                input_tokens: last.input_tokens,
                cached_input_tokens: last.cached_input_tokens,
                uncached_input_tokens: last.uncached_input_tokens,
                output_tokens: last.output_tokens,
                reasoning_tokens: Some(last.reasoning_tokens),
                total_tokens: last.total_tokens,
            },
        });
    }
    db.save_rollout_batch(
        key,
        offset,
        &serde_json::to_string(&checkpoint).map_err(|e| e.to_string())?,
        &pending,
    )?;
    Ok(latest)
}

pub async fn run(db: Arc<Db>, app: tauri::AppHandle, state: Arc<RwLock<UsageState>>) {
    let Some(dir) = crate::session_history::sessions_dir() else {
        return;
    };
    let mut latest_time = 0;
    loop {
        let mut paths = Vec::new();
        files(&dir, &mut paths);
        if let Some(root) = dir.parent() {
            files(&root.join("archived_sessions"), &mut paths);
        }
        paths.sort();
        let mut changed = false;
        for path in paths {
            let batch_db = db.clone();
            let result = tokio::task::spawn_blocking(move || import_batch(&batch_db, &path)).await;
            match result {
                Ok(Ok(Some(context))) => {
                    changed = true;
                    if context.timestamp >= latest_time {
                        latest_time = context.timestamp;
                        let mut s = state.write().await;
                        if s.snapshot.is_none() {
                            s.snapshot = Some(crate::codex_client::UsageSnapshot {
                                captured_at: chrono::Utc::now().to_rfc3339(),
                                limit_id: None,
                                limit_name: None,
                                plan_type: None,
                                rate_limit_reached_type: None,
                                credits: None,
                                windows: Vec::new(),
                                limits: Vec::new(),
                                latest_context_window: None,
                                latest_context_load_percent: None,
                                latest_last_request_tokens: None,
                            });
                        }
                        if let Some(snapshot) = &mut s.snapshot {
                            snapshot.latest_context_window = context.context_window;
                            snapshot.latest_context_load_percent =
                                context.context_window.filter(|w| *w > 0).map(|w| {
                                    (context.last.input_tokens as f64 / w as f64 * 100.0)
                                        .clamp(0.0, 100.0)
                                });
                            snapshot.latest_last_request_tokens = Some(context.last);
                        }
                    }
                }
                Ok(Err(error)) => log::warn!("Rollout import failed: {error}"),
                Err(error) => log::warn!("Rollout task failed: {error}"),
                _ => {}
            }
        }
        if changed {
            if let Err(error) = crate::usage_service::refresh_token_totals(&db, &state, &app).await
            {
                log::warn!("Token refresh failed: {error}");
            }
            let _ = app.emit("limit-history-updated", ());
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_totals_are_not_new_usage() {
        let total = DetailedTokenUsage {
            total_tokens: 100,
            ..Default::default()
        };
        assert!(delta(Some(&total), &total, &total).is_none());
    }
    #[test]
    fn inherited_and_reset_counters_only_count_the_request() {
        let previous = DetailedTokenUsage {
            total_tokens: 1000,
            ..Default::default()
        };
        let last = DetailedTokenUsage {
            total_tokens: 50,
            ..Default::default()
        };
        assert_eq!(delta(None, &previous, &last).unwrap().total_tokens, 50);
        assert_eq!(
            delta(Some(&previous), &last, &last).unwrap().total_tokens,
            50
        );
    }
}

#[cfg(test)]
mod replay_tests {
    use super::*;
    use std::io::Write;

    fn row(ts: &str, total: i64, last: i64) -> String {
        serde_json::json!({"timestamp": ts, "type": "event_msg", "payload": {"type": "token_count",
            "info": {"total_token_usage": {"input_tokens": total, "total_tokens": total},
            "last_token_usage": {"input_tokens": last, "total_tokens": last}, "model_context_window": 1000},
            "rate_limits": {"limit_id": "codex", "primary": {"window_minutes": 300, "used_percent": 10,
            "resets_at": 1800000000}, "secondary": {"window_minutes": 10080, "used_percent": 20,
            "resets_at": 1800000000}}}}).to_string()
    }

    #[test]
    fn full_replay_restart_duplicate_and_partial_line_are_safe() {
        let path = std::env::temp_dir().join(format!("rollout-{}.jsonl", uuid::Uuid::new_v4()));
        let db = Db::test_db(None);
        let first = row("2027-01-15T07:59:00Z", 100, 100);
        let second = row("2027-01-15T07:59:10Z", 150, 50);
        // No final newline means the second event is still being written.
        std::fs::write(&path, format!("{first}\n{first}\n{second}")).unwrap();
        import_batch(&db, &path).unwrap();
        assert_eq!(
            db.get_token_totals("default", 0, 0, i64::MAX, 0, i64::MAX)
                .unwrap()
                .all_time_recorded
                .total_tokens,
            100
        );
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file).unwrap();
        drop(file);
        import_batch(&db, &path).unwrap();
        import_batch(&db, &path).unwrap();
        assert_eq!(
            db.get_token_totals("default", 0, 0, i64::MAX, 0, i64::MAX)
                .unwrap()
                .all_time_recorded
                .total_tokens,
            150
        );
        assert_eq!(
            db.get_limit_history(None, None, 0, 1_800_000_001)
                .unwrap()
                .total,
            2
        );
        std::fs::remove_file(path).unwrap();
    }
}
