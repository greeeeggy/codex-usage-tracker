use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

const SUMMARY_HEAD_BYTES: u64 = 1_500_000;
const SUMMARY_TAIL_INITIAL_BYTES: u64 = 1_000_000;
const SUMMARY_TAIL_MAX_BYTES: u64 = 16_000_000;

#[derive(Clone)]
struct CachedSummary {
    file_len: u64,
    updated_at: i64,
    summary: ChatSessionSummary,
}

static SUMMARY_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedSummary>>> = OnceLock::new();

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailedTokenUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_input_tokens: i64,
    pub uncached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
}

impl DetailedTokenUsage {
    pub(crate) fn from_value(value: &Value) -> Self {
        let input_tokens = value
            .get("input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let cached_input_tokens = value
            .get("cached_input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let cache_write_input_tokens = value
            .get("cache_write_input_tokens")
            .or_else(|| value.get("cache_write_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let output_tokens = value
            .get("output_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let reasoning_tokens = value
            .get("reasoning_output_tokens")
            .or_else(|| value.get("reasoning_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let total_tokens = value
            .get("total_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(input_tokens + output_tokens);

        Self {
            input_tokens,
            cached_input_tokens,
            cache_write_input_tokens,
            uncached_input_tokens: (input_tokens - cached_input_tokens - cache_write_input_tokens)
                .max(0),
            output_tokens,
            reasoning_tokens,
            total_tokens,
        }
    }

    fn add_assign(&mut self, other: &Self) {
        self.input_tokens += other.input_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
        self.cache_write_input_tokens += other.cache_write_input_tokens;
        self.uncached_input_tokens += other.uncached_input_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_tokens += other.reasoning_tokens;
        self.total_tokens += other.total_tokens;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequestUsage {
    pub timestamp: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub usage: DetailedTokenUsage,
    pub cache_rate: f64,
    pub estimated_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionSummary {
    pub account_key: String,
    pub id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub originator: Option<String>,
    pub source: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: i64,
    pub usage: DetailedTokenUsage,
    pub cache_rate: f64,
    pub estimated_cost_usd: Option<f64>,
    pub latest_request: Option<ChatRequestUsage>,
    pub turn_count: usize,
    pub request_count: usize,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: Option<String>,
    pub role: String,
    pub text: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTurnDetail {
    pub id: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub usage: DetailedTokenUsage,
    pub cache_rate: f64,
    pub estimated_cost_usd: Option<f64>,
    pub requests: Vec<ChatRequestUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionDetail {
    pub summary: ChatSessionSummary,
    pub turns: Vec<ChatTurnDetail>,
}

pub(crate) fn invalidate_price_cache() {
    if let Some(cache) = SUMMARY_CACHE.get() {
        if let Ok(mut entries) = cache.lock() {
            entries.clear();
        }
    }
}

fn estimate_cost(
    usage: &DetailedTokenUsage,
    model: Option<&str>,
    per_request: bool,
) -> Option<f64> {
    let prices = crate::pricing::catalog();
    crate::pricing::estimate(usage, crate::pricing::lookup(&prices, model?)?, per_request)
}

fn cache_rate(usage: &DetailedTokenUsage) -> f64 {
    if usage.input_tokens <= 0 {
        0.0
    } else {
        (usage.cached_input_tokens as f64 / usage.input_tokens as f64 * 100.0).clamp(0.0, 100.0)
    }
}

pub(crate) fn sessions_dir() -> Option<PathBuf> {
    if let Some(codex_dir) = std::env::var_os("CODEX_HOME").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(codex_dir).join("sessions"));
    }
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
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
            {
                files.push(path);
            }
        }
    }
}

fn prefix(line: &str) -> &str {
    let mut end = line.len().min(768);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    &line[..end]
}

fn collapse_text(text: &str, max_chars: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }

    let mut shortened = collapsed.chars().take(max_chars).collect::<String>();
    shortened.push('…');
    shortened
}

fn message_text(payload: &Value) -> String {
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let kind = item.get("type").and_then(Value::as_str)?;
            if kind == "input_text" || kind == "output_text" {
                item.get("text").and_then(Value::as_str)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

fn message_turn_id(payload: &Value) -> Option<String> {
    payload
        .get("internal_chat_message_metadata_passthrough")
        .and_then(|metadata| metadata.get("turn_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn is_real_user_message(payload: &Value, text: &str) -> bool {
    let kinds = payload
        .get("internal_chat_message_metadata_passthrough")
        .and_then(|metadata| metadata.get("content_item_kinds"))
        .and_then(Value::as_array);

    if let Some(kinds) = kinds {
        return kinds
            .iter()
            .filter_map(Value::as_str)
            .any(|kind| kind == "user.text");
    }

    let trimmed = text.trim_start();
    !trimmed.is_empty()
        && !trimmed.starts_with("<app-context>")
        && !trimmed.starts_with("<environment_context>")
        && !trimmed.starts_with("<recommended_plugins>")
        && !trimmed.starts_with("<permissions instructions>")
        && !trimmed.starts_with("<skills_instructions>")
        && !trimmed.starts_with("<collaboration_mode>")
        && !trimmed.starts_with("<multi_agent_mode>")
}

fn parse_usage_info(payload: &Value) -> Option<(DetailedTokenUsage, DetailedTokenUsage)> {
    let info = payload.get("info")?;
    let total = DetailedTokenUsage::from_value(info.get("total_token_usage")?);
    let last = DetailedTokenUsage::from_value(info.get("last_token_usage")?);
    Some((total, last))
}

#[derive(Default)]
struct SummaryHead {
    account_key: Option<String>,
    id: Option<String>,
    title: Option<String>,
    cwd: Option<String>,
    originator: Option<String>,
    source: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    created_at: Option<String>,
}

fn read_summary_head(path: &Path) -> Result<SummaryHead, String> {
    let file = File::open(path).map_err(|error| format!("Open session failed: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut result = SummaryHead::default();
    let mut bytes_read = 0_u64;
    let mut line = String::new();

    while bytes_read < SUMMARY_HEAD_BYTES {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| format!("Read session failed: {error}"))?;
        if read == 0 {
            break;
        }
        bytes_read += read as u64;
        let line_prefix = prefix(&line);

        if line_prefix.contains("\"type\":\"session_meta\"") {
            if let Ok(row) = serde_json::from_str::<Value>(&line) {
                let payload = &row["payload"];
                result.account_key =
                    crate::accounts::from_session_metadata(payload).map(|a| a.account_key);
                result.id = payload
                    .get("session_id")
                    .or_else(|| payload.get("id"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                result.cwd = payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                result.originator = payload
                    .get("originator")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                result.source = payload
                    .get("source")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                result.created_at = payload
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .or_else(|| row.get("timestamp").and_then(Value::as_str))
                    .map(ToOwned::to_owned);
            }
        } else if line_prefix.contains("\"type\":\"turn_context\"") {
            if let Ok(row) = serde_json::from_str::<Value>(&line) {
                let payload = &row["payload"];
                if result.model.is_none() {
                    result.model = payload
                        .get("model")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                if result.reasoning_effort.is_none() {
                    result.reasoning_effort = payload
                        .pointer("/collaboration_mode/settings/reasoning_effort")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
            }
        } else if result.title.is_none()
            && line_prefix.contains("\"type\":\"response_item\"")
            && line_prefix.contains("\"type\":\"message\"")
            && line_prefix.contains("\"role\":\"user\"")
        {
            if let Ok(row) = serde_json::from_str::<Value>(&line) {
                let payload = &row["payload"];
                let text = message_text(payload);
                if is_real_user_message(payload, &text) {
                    result.title = Some(collapse_text(&text, 110));
                }
            }
        }

        if result.id.is_some()
            && result.title.is_some()
            && result.model.is_some()
            && result.reasoning_effort.is_some()
        {
            break;
        }
    }

    Ok(result)
}

struct LatestUsage {
    total: DetailedTokenUsage,
    last: DetailedTokenUsage,
    timestamp: String,
    model: Option<String>,
    reasoning_effort: Option<String>,
}

fn token_count_from_slice(slice: &str) -> Option<LatestUsage> {
    let mut latest = None;
    for line in slice.lines().rev() {
        let line_prefix = prefix(line);
        if latest.is_some() && line_prefix.contains("\"type\":\"turn_context\"") {
            let Ok(row) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let payload = &row["payload"];
            let request: &mut LatestUsage = latest.as_mut().unwrap();
            request.model = payload
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_owned);
            request.reasoning_effort = payload
                .pointer("/collaboration_mode/settings/reasoning_effort")
                .or_else(|| payload.get("effort"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            return latest.take();
        }
        if latest.is_some()
            || !line_prefix.contains("\"type\":\"event_msg\"")
            || !line_prefix.contains("\"type\":\"token_count\"")
        {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some((total, last)) = parse_usage_info(&row["payload"]) else {
            continue;
        };
        latest = Some(LatestUsage {
            total,
            last,
            timestamp: row["timestamp"].as_str().unwrap_or_default().into(),
            model: None,
            reasoning_effort: None,
        });
    }
    latest
}

fn read_latest_token_count(path: &Path) -> Result<Option<LatestUsage>, String> {
    let mut file = File::open(path).map_err(|error| format!("Open session failed: {error}"))?;
    let len = file
        .metadata()
        .map_err(|error| format!("Read session metadata failed: {error}"))?
        .len();
    if len == 0 {
        return Ok(None);
    }
    let mut span = SUMMARY_TAIL_INITIAL_BYTES.min(len);
    loop {
        file.seek(SeekFrom::Start(len - span))
            .map_err(|error| format!("Seek session failed: {error}"))?;
        let mut bytes = Vec::with_capacity(span as usize);
        file.read_to_end(&mut bytes)
            .map_err(|error| format!("Read session tail failed: {error}"))?;
        let text = String::from_utf8_lossy(&bytes);
        let complete_text = if span < len {
            text.split_once('\n').map(|(_, rest)| rest).unwrap_or("")
        } else {
            text.as_ref()
        };
        let result = token_count_from_slice(complete_text);
        if result.as_ref().is_some_and(|r| r.model.is_some()) || span == len {
            return Ok(result);
        }
        if span >= SUMMARY_TAIL_MAX_BYTES {
            // Large tool output may push the latest turn context outside the
            // tail. Stream the file instead of reusing its first model.
            file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            let mut model = None;
            let mut effort = None;
            let mut found = None;
            for line in BufReader::new(file).lines() {
                let line = line.map_err(|e| e.to_string())?;
                let p = prefix(&line);
                if p.contains("\"type\":\"turn_context\"") {
                    if let Ok(row) = serde_json::from_str::<Value>(&line) {
                        model = row["payload"]["model"].as_str().map(str::to_owned);
                        effort = row["payload"]
                            .pointer("/collaboration_mode/settings/reasoning_effort")
                            .or_else(|| row["payload"].get("effort"))
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                    }
                } else if let Some(mut usage) = token_count_from_slice(&line) {
                    usage.model = model.clone();
                    usage.reasoning_effort = effort.clone();
                    found = Some(usage);
                }
            }
            return Ok(found);
        }
        span = (span * 2).min(len).min(SUMMARY_TAIL_MAX_BYTES);
    }
}

fn file_updated_at(path: &Path) -> i64 {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn fallback_id(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown");
    stem.rsplit('-')
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("-")
}

fn fallback_title(head: &SummaryHead, id: &str) -> String {
    head.cwd
        .as_deref()
        .and_then(|cwd| Path::new(cwd).file_name())
        .and_then(|name| name.to_str())
        .map(|name| format!("{name} session"))
        .unwrap_or_else(|| format!("Codex session {}", &id[..id.len().min(8)]))
}

fn summarize_file(path: &Path) -> Result<ChatSessionSummary, String> {
    let mut head = read_summary_head(path)?;
    let id = head.id.clone().unwrap_or_else(|| fallback_id(path));
    let title = head
        .title
        .clone()
        .unwrap_or_else(|| fallback_title(&head, &id));
    let latest = read_latest_token_count(path)?;
    let (usage, latest_request) = if let Some(latest) = latest {
        if latest.model.is_some() {
            head.model = latest.model;
        }
        if latest.reasoning_effort.is_some() {
            head.reasoning_effort = latest.reasoning_effort;
        }
        let LatestUsage {
            total,
            last,
            timestamp,
            ..
        } = latest;
        let latest_request = ChatRequestUsage {
            timestamp: (!timestamp.is_empty()).then_some(timestamp),
            model: head.model.clone(),
            reasoning_effort: head.reasoning_effort.clone(),
            cache_rate: cache_rate(&last),
            estimated_cost_usd: estimate_cost(&last, head.model.as_deref(), true),
            usage: last,
        };
        (total, Some(latest_request))
    } else {
        (DetailedTokenUsage::default(), None)
    };

    Ok(ChatSessionSummary {
        account_key: head
            .account_key
            .unwrap_or_else(|| crate::accounts::LEGACY_ACCOUNT.into()),
        id,
        title,
        cwd: head.cwd,
        originator: head.originator,
        source: head.source,
        model: head.model.clone(),
        reasoning_effort: head.reasoning_effort,
        created_at: head.created_at,
        updated_at: file_updated_at(path),
        cache_rate: cache_rate(&usage),
        // The threshold multiplier applies to individual requests. A summary
        // only has cumulative totals, so use base rates until the detail scan
        // can sum exact per-request estimates.
        estimated_cost_usd: estimate_cost(&usage, head.model.as_deref(), false),
        usage,
        latest_request,
        turn_count: 0,
        request_count: 0,
        is_current: false,
    })
}

fn summarize_file_cached(path: &Path) -> Result<ChatSessionSummary, String> {
    let metadata = path
        .metadata()
        .map_err(|error| format!("Read session metadata failed: {error}"))?;
    let file_len = metadata.len();
    let updated_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let cache = SUMMARY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some(summary) = cache
        .lock()
        .ok()
        .and_then(|entries| entries.get(path).cloned())
        .filter(|entry| entry.file_len == file_len && entry.updated_at == updated_at)
        .map(|entry| entry.summary)
    {
        return Ok(summary);
    }

    let summary = summarize_file(path)?;
    if let Ok(mut entries) = cache.lock() {
        entries.insert(
            path.to_path_buf(),
            CachedSummary {
                file_len,
                updated_at,
                summary: summary.clone(),
            },
        );
    }
    Ok(summary)
}

fn rollout_files_sorted() -> Result<Vec<PathBuf>, String> {
    let directory = sessions_dir()
        .ok_or_else(|| "Could not determine the Codex sessions folder".to_string())?;
    if !directory.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    find_rollout_files(&directory, &mut files);
    files.sort_by_key(|path| std::cmp::Reverse(file_updated_at(path)));
    Ok(files)
}

pub fn list_chat_sessions() -> Result<Vec<ChatSessionSummary>, String> {
    let files = rollout_files_sorted()?;
    let available_workers = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4)
        .clamp(1, 8);
    let worker_count = available_workers.min(files.len().max(1));
    let chunk_size = files.len().div_ceil(worker_count);
    let mut summaries = Vec::with_capacity(files.len());

    std::thread::scope(|scope| {
        let handles = files
            .chunks(chunk_size.max(1))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .filter_map(|path| match summarize_file_cached(path) {
                            Ok(summary) => Some(summary),
                            Err(error) => {
                                log::warn!(
                                    "Skipping unreadable Codex session {:?}: {}",
                                    path,
                                    error
                                );
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            if let Ok(mut chunk) = handle.join() {
                summaries.append(&mut chunk);
            }
        }
    });

    summaries.sort_by_key(|summary| std::cmp::Reverse(summary.updated_at));

    if let Some(current) = summaries.first_mut() {
        current.is_current = true;
    }
    Ok(summaries)
}

pub fn current_chat_summary() -> Result<Option<ChatSessionSummary>, String> {
    let Some(path) = rollout_files_sorted()?.into_iter().next() else {
        return Ok(None);
    };
    let mut summary = summarize_file_cached(&path)?;
    summary.is_current = true;
    Ok(Some(summary))
}

pub fn list_chat_sessions_for(account_key: &str) -> Result<Vec<ChatSessionSummary>, String> {
    let mut rows = list_chat_sessions()?;
    rows.retain(|row| row.account_key == account_key);
    for (index, row) in rows.iter_mut().enumerate() {
        row.is_current = index == 0;
    }
    Ok(rows)
}

fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn find_session_file(id: &str) -> Result<PathBuf, String> {
    if !valid_session_id(id) {
        return Err("Invalid session ID".to_string());
    }

    rollout_files_sorted()?
        .into_iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(&format!("-{id}.jsonl")))
        })
        .ok_or_else(|| "Codex session not found".to_string())
}

fn get_or_create_turn(
    turns: &mut Vec<ChatTurnDetail>,
    turn_indexes: &mut HashMap<String, usize>,
    id: String,
    timestamp: Option<String>,
) -> usize {
    if let Some(index) = turn_indexes.get(&id) {
        return *index;
    }
    let index = turns.len();
    turns.push(ChatTurnDetail {
        id: id.clone(),
        started_at: timestamp,
        completed_at: None,
        model: None,
        reasoning_effort: None,
        messages: Vec::new(),
        usage: DetailedTokenUsage::default(),
        cache_rate: 0.0,
        estimated_cost_usd: None,
        requests: Vec::new(),
    });
    turn_indexes.insert(id, index);
    index
}

fn row_timestamp(row: &Value) -> Option<String> {
    row.get("timestamp")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub fn read_chat_session(id: &str) -> Result<ChatSessionDetail, String> {
    let path = find_session_file(id)?;
    let mut summary = summarize_file(&path)?;
    let file = File::open(&path).map_err(|error| format!("Open session failed: {error}"))?;
    let reader = BufReader::new(file);
    let mut turns = Vec::<ChatTurnDetail>::new();
    let mut turn_indexes = HashMap::<String, usize>::new();
    let mut current_turn_id: Option<String> = None;
    let mut last_cumulative_total: Option<i64> = None;

    for line in reader.lines() {
        let line = line.map_err(|error| format!("Read session failed: {error}"))?;
        let line_prefix = prefix(&line);
        let is_task_started = line_prefix.contains("\"type\":\"event_msg\"")
            && line_prefix.contains("\"type\":\"task_started\"");
        let is_task_complete = line_prefix.contains("\"type\":\"event_msg\"")
            && line_prefix.contains("\"type\":\"task_complete\"");
        let is_turn_context = line_prefix.contains("\"type\":\"turn_context\"");
        let is_token_count = line_prefix.contains("\"type\":\"event_msg\"")
            && line_prefix.contains("\"type\":\"token_count\"");
        let is_visible_message = line_prefix.contains("\"type\":\"response_item\"")
            && line_prefix.contains("\"type\":\"message\"")
            && (line_prefix.contains("\"role\":\"user\"")
                || line_prefix.contains("\"role\":\"assistant\""));

        if !(is_task_started
            || is_task_complete
            || is_turn_context
            || is_token_count
            || is_visible_message)
        {
            continue;
        }

        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let payload = &row["payload"];
        let timestamp = row_timestamp(&row);

        if is_task_started {
            if let Some(turn_id) = payload.get("turn_id").and_then(Value::as_str) {
                current_turn_id = Some(turn_id.to_string());
                get_or_create_turn(
                    &mut turns,
                    &mut turn_indexes,
                    turn_id.to_string(),
                    timestamp,
                );
            }
        } else if is_turn_context {
            if let Some(turn_id) = payload.get("turn_id").and_then(Value::as_str) {
                current_turn_id = Some(turn_id.to_string());
                let index = get_or_create_turn(
                    &mut turns,
                    &mut turn_indexes,
                    turn_id.to_string(),
                    timestamp,
                );
                turns[index].model = payload
                    .get("model")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                turns[index].reasoning_effort = payload
                    .pointer("/collaboration_mode/settings/reasoning_effort")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                if summary.model.is_none() {
                    summary.model = turns[index].model.clone();
                }
                if summary.reasoning_effort.is_none() {
                    summary.reasoning_effort = turns[index].reasoning_effort.clone();
                }
            }
        } else if is_visible_message {
            let role = payload
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let text = message_text(payload);
            if text.is_empty() || (role == "user" && !is_real_user_message(payload, &text)) {
                continue;
            }
            let turn_id = message_turn_id(payload)
                .or_else(|| current_turn_id.clone())
                .unwrap_or_else(|| "unassigned".to_string());
            let index =
                get_or_create_turn(&mut turns, &mut turn_indexes, turn_id, timestamp.clone());
            turns[index].messages.push(ChatMessage {
                id: payload
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                role: role.to_string(),
                text,
                timestamp,
            });
        } else if is_token_count {
            let Some((total, last)) = parse_usage_info(payload) else {
                continue;
            };
            if last_cumulative_total == Some(total.total_tokens) {
                continue;
            }
            last_cumulative_total = Some(total.total_tokens);
            summary.usage = total;

            let turn_id = current_turn_id
                .clone()
                .unwrap_or_else(|| "unassigned".to_string());
            let index =
                get_or_create_turn(&mut turns, &mut turn_indexes, turn_id, timestamp.clone());
            let model = turns[index].model.clone().or_else(|| summary.model.clone());
            let effort = turns[index]
                .reasoning_effort
                .clone()
                .or_else(|| summary.reasoning_effort.clone());
            turns[index].requests.push(ChatRequestUsage {
                timestamp,
                model: model.clone(),
                reasoning_effort: effort,
                cache_rate: cache_rate(&last),
                estimated_cost_usd: estimate_cost(&last, model.as_deref(), true),
                usage: last,
            });
        } else if is_task_complete {
            let turn_id = payload
                .get("turn_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| current_turn_id.clone());
            if let Some(turn_id) = turn_id {
                let index =
                    get_or_create_turn(&mut turns, &mut turn_indexes, turn_id, timestamp.clone());
                turns[index].completed_at = timestamp;
            }
        }
    }

    turns.retain(|turn| !turn.messages.is_empty() || !turn.requests.is_empty());
    let aggregate_cost_estimate = summary.estimated_cost_usd;
    let mut exact_cost = 0.0;
    let mut all_requests_priced = true;
    let mut request_count = 0;
    for turn in &mut turns {
        for request in &turn.requests {
            turn.usage.add_assign(&request.usage);
            request_count += 1;
            if let Some(cost) = request.estimated_cost_usd {
                exact_cost += cost;
            } else {
                all_requests_priced = false;
            }
        }
        turn.cache_rate = cache_rate(&turn.usage);
        turn.estimated_cost_usd = if turn
            .requests
            .iter()
            .all(|request| request.estimated_cost_usd.is_some())
            && !turn.requests.is_empty()
        {
            Some(
                turn.requests
                    .iter()
                    .filter_map(|request| request.estimated_cost_usd)
                    .sum(),
            )
        } else {
            None
        };
    }

    summary.turn_count = turns.len();
    summary.request_count = request_count;
    summary.cache_rate = cache_rate(&summary.usage);
    summary.estimated_cost_usd = if request_count > 0 && all_requests_priced {
        Some(exact_cost)
    } else if request_count == 0 {
        aggregate_cost_estimate
    } else {
        None
    };
    summary.latest_request = turns
        .iter()
        .rev()
        .find_map(|turn| turn.requests.last().cloned());
    summary.is_current = rollout_files_sorted()?
        .first()
        .is_some_and(|current| current == &path);

    Ok(ChatSessionDetail { summary, turns })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_sol_cache_rate_and_cost() {
        let usage = DetailedTokenUsage {
            input_tokens: 256,
            cached_input_tokens: 251,
            cache_write_input_tokens: 0,
            uncached_input_tokens: 5,
            output_tokens: 100,
            reasoning_tokens: 30,
            total_tokens: 356,
        };

        assert!((cache_rate(&usage) - 98.046875).abs() < 0.0001);
    }

    #[test]
    fn parses_cache_write_tokens() {
        let value = serde_json::json!({
            "input_tokens": 1_000,
            "cached_input_tokens": 800,
            "cache_write_input_tokens": 100,
            "output_tokens": 50,
            "reasoning_output_tokens": 20,
            "total_tokens": 1_050
        });
        let usage = DetailedTokenUsage::from_value(&value);
        assert_eq!(usage.cache_write_input_tokens, 100);
        assert_eq!(usage.uncached_input_tokens, 100);
        assert_eq!(usage.reasoning_tokens, 20);
    }

    #[test]
    fn accepts_only_real_user_text_when_metadata_is_present() {
        let instructions = serde_json::json!({
            "internal_chat_message_metadata_passthrough": {
                "content_item_kinds": ["environments.environment_context"]
            }
        });
        let user = serde_json::json!({
            "internal_chat_message_metadata_passthrough": {
                "content_item_kinds": ["user.text"]
            }
        });
        assert!(!is_real_user_message(&instructions, "hidden context"));
        assert!(is_real_user_message(&user, "show my usage"));
    }
}

#[cfg(test)]
mod current_model_tests {
    use super::*;
    #[test]
    fn latest_request_uses_its_model_after_a_model_switch() {
        let text = concat!(
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"old-model\"}}\n",
            "{\"timestamp\":\"2026-10-02T12:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":100},\"last_token_usage\":{\"total_tokens\":100}}}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"brand-new-model\",\"effort\":\"high\"}}\n",
            "{\"timestamp\":\"2026-10-02T13:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":200},\"last_token_usage\":{\"total_tokens\":100}}}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\","
        );
        let latest = token_count_from_slice(text).unwrap();
        assert_eq!(latest.model.as_deref(), Some("brand-new-model"));
        assert_eq!(latest.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(latest.total.total_tokens, 200);
    }
}
