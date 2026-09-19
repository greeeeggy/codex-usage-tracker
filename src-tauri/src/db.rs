use chrono::{Datelike, Local, TimeZone};
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::Manager;

/// Token event data model for database persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEvent {
    pub id: String,
    pub account_key: String,
    pub captured_at: i64,
    pub client_type: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub project_path_hash: Option<String>,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: Option<i64>,
    pub total_tokens: i64,
    pub event_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenBreakdown {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub uncached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: Option<i64>,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenTotals {
    pub current_session: TokenBreakdown,
    pub five_hour_window: TokenBreakdown,
    pub weekly_window: TokenBreakdown,
    pub today: TokenBreakdown,
    pub current_month: TokenBreakdown,
    pub all_time_recorded: TokenBreakdown,
}

/// A single quota sample data point for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSampleRow {
    pub captured_at: i64,
    pub window_kind: String,
    pub used_percent: f64,
    pub remaining_percent: f64,
}

/// Usage deltas derived from quota samples over different time periods
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageDeltas {
    /// Change in used_percent during the current session (since app start)
    pub session_delta: f64,
    /// Change in used_percent today (since midnight)
    pub today_delta: f64,
    /// Peak used_percent value seen in the last hour
    pub peak_hour_used: f64,
    /// Number of distinct monitoring sessions (connect→dormant transitions) today
    pub sessions_today: i64,
    /// Longest continuous monitoring stretch in minutes today
    pub longest_session_minutes: i64,
}

/// An application event for the event log
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEvent {
    pub event_type: String,
    pub label: String,
    pub timestamp: String,
    pub description: Option<String>,
}

pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn new(app_handle: &tauri::AppHandle) -> Result<Self, String> {
        let app_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {}", e))?;

        if !app_dir.exists() {
            std::fs::create_dir_all(&app_dir)
                .map_err(|e| format!("Failed to create app data dir: {}", e))?;
        }

        let db_path = app_dir.join("codex_meter.db");
        log::info!("Initializing SQLite database at: {:?}", db_path);

        let conn =
            Connection::open(&db_path).map_err(|e| format!("Failed to open database: {}", e))?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        db.initialize_schema()?;

        Ok(db)
    }

    fn initialize_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS accounts (
                account_key TEXT PRIMARY KEY,
                plan_type TEXT,
                first_seen_at INTEGER NOT NULL,
                last_seen_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS quota_samples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_key TEXT NOT NULL,
                captured_at INTEGER NOT NULL,
                window_kind TEXT NOT NULL,
                duration_minutes INTEGER,
                used_percent REAL NOT NULL,
                remaining_percent REAL NOT NULL,
                resets_at INTEGER,
                source TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS token_events (
                id TEXT PRIMARY KEY,
                account_key TEXT NOT NULL,
                captured_at INTEGER NOT NULL,
                client_type TEXT NOT NULL,
                thread_id TEXT,
                turn_id TEXT,
                project_path_hash TEXT,
                model TEXT,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                cached_input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                reasoning_tokens INTEGER,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                event_type TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS quota_periods (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_key TEXT NOT NULL,
                window_kind TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                resets_at INTEGER NOT NULL,
                final_used_percent REAL,
                detected_reset_at INTEGER
            );

            CREATE TABLE IF NOT EXISTS notification_states (
                account_key TEXT NOT NULL,
                window_kind TEXT NOT NULL,
                resets_at INTEGER NOT NULL,
                threshold INTEGER NOT NULL,
                fired_at INTEGER NOT NULL,
                PRIMARY KEY (account_key, window_kind, resets_at, threshold)
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS app_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                label TEXT NOT NULL,
                captured_at INTEGER NOT NULL,
                description TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_token_events_account ON token_events(account_key);
            CREATE INDEX IF NOT EXISTS idx_token_events_captured ON token_events(captured_at);
            CREATE INDEX IF NOT EXISTS idx_quota_samples_captured ON quota_samples(captured_at);
            CREATE INDEX IF NOT EXISTS idx_quota_samples_kind ON quota_samples(window_kind, captured_at);
            CREATE INDEX IF NOT EXISTS idx_app_events_captured ON app_events(captured_at);
            "
        ).map_err(|e| format!("Failed to initialize schema: {}", e))?;

        Ok(())
    }

    // ── Quota Samples ──────────────────────────────────────────────────

    /// Persist a single rate-limit window data point.
    pub fn insert_quota_sample(
        &self,
        account_key: &str,
        captured_at: i64,
        window_kind: &str,
        duration_minutes: Option<u64>,
        used_percent: f64,
        remaining_percent: f64,
        resets_at: Option<i64>,
        source: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO quota_samples (account_key, captured_at, window_kind, duration_minutes, used_percent, remaining_percent, resets_at, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                account_key,
                captured_at,
                window_kind,
                duration_minutes.map(|v| v as i64),
                used_percent,
                remaining_percent,
                resets_at,
                source,
            ],
        ).map_err(|e| format!("Failed to insert quota sample: {}", e))?;
        Ok(())
    }

    /// Get recent quota samples for a window kind within a time range.
    /// Used to populate charts. Returns at most `limit` samples.
    pub fn get_recent_quota_samples(
        &self,
        window_kind: &str,
        since_ts: i64,
        limit: i64,
    ) -> Result<Vec<QuotaSampleRow>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT captured_at, window_kind, used_percent, remaining_percent
             FROM (
                 SELECT captured_at, window_kind, used_percent, remaining_percent
                 FROM quota_samples
                 WHERE window_kind = ?1 AND captured_at >= ?2
                 ORDER BY captured_at DESC
                 LIMIT ?3
             )
             ORDER BY captured_at ASC",
            )
            .map_err(|e| format!("Prepare failed: {}", e))?;

        let rows = stmt
            .query_map(params![window_kind, since_ts, limit], |row| {
                Ok(QuotaSampleRow {
                    captured_at: row.get(0)?,
                    window_kind: row.get(1)?,
                    used_percent: row.get(2)?,
                    remaining_percent: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query failed: {}", e))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Row error: {}", e))?);
        }
        Ok(results)
    }

    /// Compute usage deltas from quota samples for a given window kind.
    pub fn get_usage_deltas(
        &self,
        window_kind: &str,
        session_start_ts: i64,
    ) -> Result<UsageDeltas, String> {
        let conn = self.conn.lock().unwrap();
        let now = Local::now();

        let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let today_start_ts = Local.from_local_datetime(&today_start).unwrap().timestamp();
        let one_hour_ago = chrono::Utc::now().timestamp() - 3600;

        // Session delta: difference between first and latest sample since session start
        let session_delta = Self::calc_delta(&conn, window_kind, session_start_ts)?;

        // Today delta: difference between first and latest sample since midnight
        let today_delta = Self::calc_delta(&conn, window_kind, today_start_ts)?;

        // Peak hour: max used_percent in the last hour
        let peak_hour_used: f64 = conn.query_row(
            "SELECT COALESCE(MAX(used_percent), 0.0) FROM quota_samples WHERE window_kind = ?1 AND captured_at >= ?2",
            params![window_kind, one_hour_ago],
            |row| row.get(0),
        ).unwrap_or(0.0);

        // Sessions today: count distinct monitoring sessions by counting app_events of type session_started
        let sessions_today: i64 = conn.query_row(
            "SELECT COUNT(*) FROM app_events WHERE event_type = 'session_started' AND captured_at >= ?1",
            params![today_start_ts],
            |row| row.get(0),
        ).unwrap_or(0);

        // Longest session: find the longest gap between session_started and session_ended today
        let longest_session_minutes = Self::calc_longest_session(&conn, today_start_ts)?;

        Ok(UsageDeltas {
            session_delta,
            today_delta,
            peak_hour_used,
            sessions_today,
            longest_session_minutes,
        })
    }

    fn calc_delta(conn: &Connection, window_kind: &str, since_ts: i64) -> Result<f64, String> {
        let first: Option<f64> = conn.query_row(
            "SELECT used_percent FROM quota_samples WHERE window_kind = ?1 AND captured_at >= ?2 ORDER BY captured_at ASC LIMIT 1",
            params![window_kind, since_ts],
            |row| row.get(0),
        ).ok();

        let latest: Option<f64> = conn.query_row(
            "SELECT used_percent FROM quota_samples WHERE window_kind = ?1 AND captured_at >= ?2 ORDER BY captured_at DESC LIMIT 1",
            params![window_kind, since_ts],
            |row| row.get(0),
        ).ok();

        match (first, latest) {
            (Some(f), Some(l)) => Ok((l - f).max(0.0)),
            _ => Ok(0.0),
        }
    }

    fn calc_longest_session(conn: &Connection, since_ts: i64) -> Result<i64, String> {
        // Get all session start/end events today, ordered by time
        let mut stmt = conn
            .prepare(
                "SELECT event_type, captured_at FROM app_events
             WHERE event_type IN ('session_started', 'session_ended') AND captured_at >= ?1
             ORDER BY captured_at ASC",
            )
            .map_err(|e| format!("Prepare failed: {}", e))?;

        let events: Vec<(String, i64)> = stmt
            .query_map(params![since_ts], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| format!("Query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        let mut longest = 0i64;
        let mut session_start: Option<i64> = None;

        for (event_type, ts) in &events {
            if event_type == "session_started" {
                session_start = Some(*ts);
            } else if event_type == "session_ended" {
                if let Some(start) = session_start {
                    let duration = (*ts - start) / 60;
                    if duration > longest {
                        longest = duration;
                    }
                    session_start = None;
                }
            }
        }

        // If there's an open session, count until now
        if let Some(start) = session_start {
            let duration = (chrono::Utc::now().timestamp() - start) / 60;
            if duration > longest {
                longest = duration;
            }
        }

        Ok(longest)
    }

    // ── App Events ─────────────────────────────────────────────────────

    /// Record an application event (state transitions, connections, etc.)
    pub fn insert_app_event(
        &self,
        event_type: &str,
        label: &str,
        description: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO app_events (event_type, label, captured_at, description) VALUES (?1, ?2, ?3, ?4)",
            params![event_type, label, now, description],
        ).map_err(|e| format!("Failed to insert app event: {}", e))?;
        Ok(())
    }

    /// Get recent app events, newest first. Returns at most `limit` events.
    pub fn get_recent_events(&self, limit: i64) -> Result<Vec<AppEvent>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT event_type, label, captured_at, description
             FROM app_events
             ORDER BY captured_at DESC
             LIMIT ?1",
            )
            .map_err(|e| format!("Prepare failed: {}", e))?;

        let rows = stmt
            .query_map(params![limit], |row| {
                let ts: i64 = row.get(2)?;
                let dt = chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.with_timezone(&Local).format("%H:%M:%S").to_string())
                    .unwrap_or_else(|| "—".to_string());

                Ok(AppEvent {
                    event_type: row.get(0)?,
                    label: row.get(1)?,
                    timestamp: dt,
                    description: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query failed: {}", e))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Row error: {}", e))?);
        }
        Ok(results)
    }

    // ── Token Events (existing) ────────────────────────────────────────

    /// Insert or replace a final turn token event.
    pub fn insert_token_event(&self, event: &TokenEvent) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();

        let mut event_id = event.id.clone();
        if let (Some(thread), Some(turn)) = (&event.thread_id, &event.turn_id) {
            event_id = format!("{}:{}:{}", event.account_key, thread, turn);
        }

        conn.execute(
            "REPLACE INTO token_events (
                id, account_key, captured_at, client_type, thread_id, turn_id, project_path_hash, model,
                input_tokens, cached_input_tokens, output_tokens, reasoning_tokens, total_tokens, event_type
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                event_id,
                event.account_key,
                event.captured_at,
                event.client_type,
                event.thread_id,
                event.turn_id,
                event.project_path_hash,
                event.model,
                event.input_tokens,
                event.cached_input_tokens,
                event.output_tokens,
                event.reasoning_tokens,
                event.total_tokens,
                event.event_type,
            ],
        ).map_err(|e| format!("Failed to insert token event: {}", e))?;

        Ok(())
    }

    fn calculate_breakdown(
        conn: &Connection,
        account_key: &str,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<TokenBreakdown, String> {
        let mut stmt = conn
            .prepare(
                "SELECT
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(cached_input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(reasoning_tokens), 0),
                COALESCE(SUM(total_tokens), 0)
             FROM token_events
             WHERE account_key = ?1 AND captured_at >= ?2 AND captured_at <= ?3",
            )
            .map_err(|e| format!("Prepare query failed: {}", e))?;

        let mut rows = stmt
            .query(params![account_key, start_ts, end_ts])
            .map_err(|e| format!("Query failed: {}", e))?;
        if let Some(row) = rows.next().map_err(|e| format!("Next row failed: {}", e))? {
            let input_tokens: i64 = row.get(0).unwrap_or(0);
            let cached_input_tokens: i64 = row.get(1).unwrap_or(0);
            let output_tokens: i64 = row.get(2).unwrap_or(0);
            let reasoning_tokens: i64 = row.get(3).unwrap_or(0);
            let total_tokens: i64 = row.get(4).unwrap_or(0);

            return Ok(TokenBreakdown {
                input_tokens,
                cached_input_tokens,
                uncached_input_tokens: std::cmp::max(0, input_tokens - cached_input_tokens),
                output_tokens,
                reasoning_tokens: Some(reasoning_tokens),
                total_tokens,
            });
        }

        Ok(TokenBreakdown::default())
    }

    fn calculate_latest_chat_breakdown(
        conn: &Connection,
        account_key: &str,
    ) -> Result<TokenBreakdown, String> {
        let thread_id = {
            let mut stmt = conn
                .prepare(
                    "SELECT thread_id
                     FROM token_events
                     WHERE account_key = ?1
                       AND thread_id IS NOT NULL
                       AND event_type = 'local_rollout'
                     ORDER BY captured_at DESC
                     LIMIT 1",
                )
                .map_err(|e| format!("Prepare latest chat query failed: {}", e))?;
            let mut rows = stmt
                .query(params![account_key])
                .map_err(|e| format!("Latest chat query failed: {}", e))?;
            rows.next()
                .map_err(|e| format!("Latest chat row failed: {}", e))?
                .and_then(|row| row.get::<_, String>(0).ok())
        };

        let Some(thread_id) = thread_id else {
            return Ok(TokenBreakdown::default());
        };

        let mut stmt = conn
            .prepare(
                "SELECT
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(cached_input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(reasoning_tokens), 0),
                    COALESCE(SUM(total_tokens), 0)
                 FROM token_events
                 WHERE account_key = ?1 AND thread_id = ?2",
            )
            .map_err(|e| format!("Prepare current chat query failed: {}", e))?;
        let mut rows = stmt
            .query(params![account_key, thread_id])
            .map_err(|e| format!("Current chat query failed: {}", e))?;
        if let Some(row) = rows
            .next()
            .map_err(|e| format!("Current chat row failed: {}", e))?
        {
            let input_tokens = row.get(0).unwrap_or(0);
            let cached_input_tokens = row.get(1).unwrap_or(0);
            let output_tokens = row.get(2).unwrap_or(0);
            let reasoning_tokens = row.get(3).unwrap_or(0);
            let total_tokens = row.get(4).unwrap_or(0);
            return Ok(TokenBreakdown {
                input_tokens,
                cached_input_tokens,
                uncached_input_tokens: std::cmp::max(0, input_tokens - cached_input_tokens),
                output_tokens,
                reasoning_tokens: Some(reasoning_tokens),
                total_tokens,
            });
        }

        Ok(TokenBreakdown::default())
    }

    pub fn get_token_totals(
        &self,
        account_key: &str,
        _session_start_ts: i64,
        five_hour_start_ts: i64,
        five_hour_end_ts: i64,
        weekly_start_ts: i64,
        weekly_end_ts: i64,
    ) -> Result<TokenTotals, String> {
        let conn = self.conn.lock().unwrap();

        let now = Local::now();

        let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let today_start_ts = Local.from_local_datetime(&today_start).unwrap().timestamp();

        let month_start = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let month_start_ts = Local.from_local_datetime(&month_start).unwrap().timestamp();

        // "Current session" is the active Codex chat, not the lifetime of this
        // meter process. The previous process-start boundary made this stay at
        // zero when the dashboard launched after a chat had already begun.
        let current_session = Self::calculate_latest_chat_breakdown(&conn, account_key)?;
        let five_hour_window =
            Self::calculate_breakdown(&conn, account_key, five_hour_start_ts, five_hour_end_ts)?;
        let weekly_window =
            Self::calculate_breakdown(&conn, account_key, weekly_start_ts, weekly_end_ts)?;
        let today = Self::calculate_breakdown(&conn, account_key, today_start_ts, i64::MAX)?;
        let current_month =
            Self::calculate_breakdown(&conn, account_key, month_start_ts, i64::MAX)?;
        let all_time_recorded = Self::calculate_breakdown(&conn, account_key, 0, i64::MAX)?;

        Ok(TokenTotals {
            current_session,
            five_hour_window,
            weekly_window,
            today,
            current_month,
            all_time_recorded,
        })
    }

    pub fn get_session_cumulative_totals(
        &self,
        session_id: &str,
    ) -> Result<TokenBreakdown, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(cached_input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(reasoning_tokens), 0),
                COALESCE(SUM(total_tokens), 0)
             FROM token_events
             WHERE thread_id = ?1",
            )
            .map_err(|e| format!("Prepare query failed: {}", e))?;

        let mut rows = stmt
            .query(params![session_id])
            .map_err(|e| format!("Query failed: {}", e))?;
        if let Some(row) = rows.next().map_err(|e| format!("Next row failed: {}", e))? {
            let input_tokens: i64 = row.get(0).unwrap_or(0);
            let cached_input_tokens: i64 = row.get(1).unwrap_or(0);
            let output_tokens: i64 = row.get(2).unwrap_or(0);
            let reasoning_tokens: i64 = row.get(3).unwrap_or(0);
            let total_tokens: i64 = row.get(4).unwrap_or(0);

            return Ok(TokenBreakdown {
                input_tokens,
                cached_input_tokens,
                uncached_input_tokens: std::cmp::max(0, input_tokens - cached_input_tokens),
                output_tokens,
                reasoning_tokens: Some(reasoning_tokens),
                total_tokens,
            });
        }

        Ok(TokenBreakdown::default())
    }
}
