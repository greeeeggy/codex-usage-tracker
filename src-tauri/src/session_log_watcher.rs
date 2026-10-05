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
    #[serde(default)]
    account_key: Option<String>,
    previous: Option<DetailedTokenUsage>,
    model: Option<String>,
    #[serde(default)]
    turn_id: Option<String>,
}

struct LiveContext {
    account_key: String,
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
    if checkpoint.account_key.is_none() {
        let mut header = String::new();
        BufReader::new(File::open(path).map_err(|e| e.to_string())?)
            .read_line(&mut header)
            .map_err(|e| e.to_string())?;
        let profile = serde_json::from_str::<Value>(&header)
            .ok()
            .filter(|row| row["type"] == "session_meta")
            .and_then(|row| crate::accounts::from_session_metadata(&row["payload"]));
        if let Some(profile) = profile {
            db.remember_account(&profile)?;
            db.assign_rollout_account(key, &profile.account_key)?;
            checkpoint = Checkpoint {
                account_key: Some(profile.account_key),
                ..Default::default()
            };
            // Rebuild identified observations once, preserving global request IDs.
            offset = 0;
        } else {
            checkpoint.account_key = Some(crate::accounts::LEGACY_ACCOUNT.into());
            db.save_rollout_batch(
                key,
                offset,
                &serde_json::to_string(&checkpoint).map_err(|e| e.to_string())?,
                &[],
            )?;
        }
    }
    if offset == len {
        return Ok(None);
    }
    if offset > len {
        offset = 0;
        checkpoint = Checkpoint {
            account_key: checkpoint.account_key.clone(),
            ..Default::default()
        };
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
        if !["token_count", "turn_context", "task_started"]
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
            if let Some(turn) = payload["turn_id"].as_str() {
                checkpoint.turn_id = Some(turn.to_owned());
            }
            continue;
        }
        if row["type"] == "event_msg" && payload["type"] == "task_started" {
            checkpoint.turn_id = payload["turn_id"].as_str().map(str::to_owned);
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
        let mut rate_snapshot =
            crate::codex_client::snapshot_from_rollout(&payload["rate_limits"], timestamp).filter(
                |s| {
                    s.windows
                        .iter()
                        .any(|w| w.duration_minutes.is_some_and(|m| m > 0))
                },
            );
        if let Some(snapshot) = &mut rate_snapshot {
            snapshot.account_key = checkpoint.account_key.clone();
            crate::limit_history::persist_snapshot(db, snapshot);
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
            let limit_id = rate_snapshot
                .as_ref()
                .map(|s| s.limit_id.as_deref().unwrap_or("codex"))
                .unwrap_or("unattributed")
                .to_string();
            pending.push((
                TokenEvent {
                    // Forks can copy a parent's token rows and rewrite their
                    // timestamps. A shared turn/counter identity counts that
                    // request once across both files.
                    id: checkpoint.turn_id.as_ref().map_or_else(
                        || format!("{key}:{row_offset}"),
                        |turn| {
                            format!(
                                "turn:{turn}:{}:{}:{}:{}:{}:{}",
                                total.input_tokens,
                                total.cached_input_tokens,
                                total.cache_write_input_tokens,
                                total.output_tokens,
                                total.reasoning_tokens,
                                total.total_tokens
                            )
                        },
                    ),
                    account_key: checkpoint
                        .account_key
                        .clone()
                        .unwrap_or_else(|| crate::accounts::LEGACY_ACCOUNT.into()),
                    captured_at: captured,
                    client_type: "local_session".into(),
                    thread_id: Some(key.into()),
                    turn_id: checkpoint.turn_id.clone(),
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
            account_key: checkpoint
                .account_key
                .clone()
                .unwrap_or_else(|| crate::accounts::LEGACY_ACCOUNT.into()),
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
    let mut latest_times = std::collections::HashMap::new();
    loop {
        let mut paths = Vec::new();
        files(&dir, &mut paths);
        if let Some(root) = dir.parent() {
            files(&root.join("archived_sessions"), &mut paths);
        }
        paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
        let mut changed = false;
        for path in paths {
            let batch_db = db.clone();
            let result = tokio::task::spawn_blocking(move || import_batch(&batch_db, &path)).await;
            match result {
                Ok(Ok(Some(context))) => {
                    changed = true;
                    let latest_time = latest_times.entry(context.account_key.clone()).or_insert(0);
                    if context.timestamp >= *latest_time {
                        *latest_time = context.timestamp;
                        let mut s = state.write().await;
                        if s.active_account.as_ref().map(|a| &a.account_key)
                            != Some(&context.account_key)
                        {
                            continue;
                        }
                        if s.snapshot.is_none() {
                            s.snapshot = Some(crate::codex_client::UsageSnapshot {
                                account_key: Some(context.account_key),
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
            let context = crate::accounts::AccountContext {
                active_account: state.read().await.active_account.clone(),
                accounts: db.list_accounts().unwrap_or_default(),
            };
            let _ = app.emit("accounts-updated", &context);
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
    fn identified_old_checkpoint_recovers_ownership_once_without_losing_unknown_history() {
        let path = std::env::temp_dir().join(format!("rollout-{}.jsonl", uuid::Uuid::new_v4()));
        let key = path.file_stem().unwrap().to_str().unwrap();
        let header = serde_json::json!({"type":"session_meta","payload":{"creator_account_id":"workspace-a","creator_user_id":"user-a"}}).to_string();
        let content = format!("{header}\n{}\n", row("2027-01-15T07:59:00Z", 100, 100));
        std::fs::write(&path, &content).unwrap();
        let db = Db::test_db(None);
        let old_event = TokenEvent {
            id: format!("{key}:{}", header.len() + 1),
            account_key: "default".into(),
            captured_at: 1799999940,
            client_type: "local_session".into(),
            thread_id: Some(key.into()),
            turn_id: None,
            project_path_hash: None,
            model: None,
            input_tokens: 100,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: Some(0),
            total_tokens: 100,
            event_type: "local_rollout".into(),
        };
        db.save_rollout_batch(
            key,
            content.len() as u64,
            "{}",
            &[(old_event, "codex".into())],
        )
        .unwrap();
        let profile = crate::accounts::identity("workspace-a", "user-a");
        import_batch(&db, &path).unwrap();
        import_batch(&db, &path).unwrap();
        assert_eq!(
            db.get_token_totals(&profile.account_key, 0, 0, i64::MAX, 0, i64::MAX)
                .unwrap()
                .all_time_recorded
                .total_tokens,
            100
        );
        assert_eq!(
            db.get_token_totals("default", 0, 0, i64::MAX, 0, i64::MAX)
                .unwrap()
                .all_time_recorded
                .total_tokens,
            0
        );
        assert_eq!(
            db.get_limit_history_for(&profile.account_key, None, None, 0, 1800000001)
                .unwrap()
                .total,
            2
        );
        let unknown = std::env::temp_dir().join(format!("rollout-{}.jsonl", uuid::Uuid::new_v4()));
        std::fs::write(
            &unknown,
            format!("{}\n", row("2027-01-15T07:59:00Z", 50, 50)),
        )
        .unwrap();
        import_batch(&db, &unknown).unwrap();
        assert_eq!(
            db.get_token_totals("default", 0, 0, i64::MAX, 0, i64::MAX)
                .unwrap()
                .all_time_recorded
                .total_tokens,
            50
        );
        assert_eq!(
            db.get_token_totals(&profile.account_key, 0, 0, i64::MAX, 0, i64::MAX)
                .unwrap()
                .all_time_recorded
                .total_tokens,
            100
        );
        for file in [path, unknown] {
            std::fs::remove_file(file).unwrap();
        }
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

    #[test]
    fn copied_fork_history_counts_once_even_when_child_is_imported_first() {
        let turn = uuid::Uuid::new_v4().to_string();
        let context = |id: &str| {
            serde_json::json!({"type": "turn_context",
            "payload": {"turn_id": id, "model": "future-model"}})
            .to_string()
        };
        let parent = std::env::temp_dir().join(format!("rollout-{}.jsonl", uuid::Uuid::new_v4()));
        let child = std::env::temp_dir().join(format!("rollout-{}.jsonl", uuid::Uuid::new_v4()));
        let independent =
            std::env::temp_dir().join(format!("rollout-{}.jsonl", uuid::Uuid::new_v4()));
        std::fs::write(
            &parent,
            format!(
                "{}\n{}\n",
                context(&turn),
                row("2027-01-15T07:59:00Z", 100, 100)
            ),
        )
        .unwrap();
        std::fs::write(
            &child,
            format!(
                "{}\n{}\n{}\n{}\n",
                context(&turn),
                row("2027-01-15T07:59:20Z", 100, 100),
                context(&uuid::Uuid::new_v4().to_string()),
                row("2027-01-15T07:59:30Z", 150, 50)
            ),
        )
        .unwrap();
        std::fs::write(
            &independent,
            format!(
                "{}\n{}\n",
                context(&uuid::Uuid::new_v4().to_string()),
                row("2027-01-15T07:59:40Z", 100, 100)
            ),
        )
        .unwrap();
        for child_first in [true, false] {
            let db = Db::test_db(None);
            let order = if child_first {
                [&child, &parent]
            } else {
                [&parent, &child]
            };
            for path in order {
                import_batch(&db, path).unwrap();
            }
            import_batch(&db, &independent).unwrap();
            import_batch(&db, &child).unwrap();
            assert_eq!(
                db.get_token_totals("default", 0, 0, i64::MAX, 0, i64::MAX)
                    .unwrap()
                    .all_time_recorded
                    .total_tokens,
                250
            );
            // A copied row's rewritten timestamp cannot move the original
            // request into a later quota period.
            assert_eq!(
                db.get_token_totals("default", 0, 0, 1_799_999_950, 0, i64::MAX)
                    .unwrap()
                    .five_hour_window
                    .total_tokens,
                100
            );
            assert!(db
                .get_limit_history(None, None, 0, 1_800_000_001)
                .unwrap()
                .periods
                .iter()
                .all(|period| period.tokens.total_tokens == 250));
        }
        for path in [parent, child, independent] {
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn cross_account_forks_keep_original_requests_with_the_original_account() {
        let parent = std::env::temp_dir().join(format!("rollout-{}.jsonl", uuid::Uuid::new_v4()));
        let child = std::env::temp_dir().join(format!("rollout-{}.jsonl", uuid::Uuid::new_v4()));
        let header = |account: &str| {
            serde_json::json!({"type":"session_meta","payload":{"creator_account_id":account,"creator_user_id":"user"}}).to_string()
        };
        let context = |turn: &str| {
            serde_json::json!({"type":"turn_context","payload":{"turn_id":turn}}).to_string()
        };
        let original_turn = uuid::Uuid::new_v4().to_string();
        std::fs::write(
            &parent,
            format!(
                "{}\n{}\n{}\n",
                header("a"),
                context(&original_turn),
                row("2027-01-15T07:59:00Z", 100, 100)
            ),
        )
        .unwrap();
        std::fs::write(
            &child,
            format!(
                "{}\n{}\n{}\n{}\n{}\n",
                header("b"),
                context(&original_turn),
                row("2027-01-15T07:59:20Z", 100, 100),
                context(&uuid::Uuid::new_v4().to_string()),
                row("2027-01-15T07:59:30Z", 150, 50)
            ),
        )
        .unwrap();
        for order in [[&parent, &child], [&child, &parent]] {
            let db = Db::test_db(None);
            for path in order {
                import_batch(&db, path).unwrap();
            }
            for (account, total) in [("a", 100), ("b", 50)] {
                let key = crate::accounts::identity(account, "user").account_key;
                assert_eq!(
                    db.get_token_totals(&key, 0, 0, i64::MAX, 0, i64::MAX)
                        .unwrap()
                        .all_time_recorded
                        .total_tokens,
                    total
                );
            }
        }
        for path in [parent, child] {
            std::fs::remove_file(path).unwrap();
        }
    }
}
