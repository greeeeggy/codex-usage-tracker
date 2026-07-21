use chrono::{Local, TimeZone, Datelike};
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

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;

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

            CREATE INDEX IF NOT EXISTS idx_token_events_account ON token_events(account_key);
            CREATE INDEX IF NOT EXISTS idx_token_events_captured ON token_events(captured_at);
            "
        ).map_err(|e| format!("Failed to initialize schema: {}", e))?;

        Ok(())
    }

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

    fn calculate_breakdown(conn: &Connection, account_key: &str, start_ts: i64, end_ts: i64) -> Result<TokenBreakdown, String> {
        let mut stmt = conn.prepare(
            "SELECT 
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(cached_input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(reasoning_tokens), 0),
                COALESCE(SUM(total_tokens), 0)
             FROM token_events
             WHERE account_key = ?1 AND captured_at >= ?2 AND captured_at <= ?3"
        ).map_err(|e| format!("Prepare query failed: {}", e))?;

        let mut rows = stmt.query(params![account_key, start_ts, end_ts]).map_err(|e| format!("Query failed: {}", e))?;
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

    pub fn get_token_totals(
        &self, 
        account_key: &str,
        session_start_ts: i64,
        five_hour_start_ts: i64,
        five_hour_end_ts: i64,
        weekly_start_ts: i64,
        weekly_end_ts: i64
    ) -> Result<TokenTotals, String> {
        let conn = self.conn.lock().unwrap();

        let now = Local::now();
        
        let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let today_start_ts = Local.from_local_datetime(&today_start).unwrap().timestamp();
        
        let month_start = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
        let month_start_ts = Local.from_local_datetime(&month_start).unwrap().timestamp();

        let current_session = Self::calculate_breakdown(&conn, account_key, session_start_ts, i64::MAX)?;
        let five_hour_window = Self::calculate_breakdown(&conn, account_key, five_hour_start_ts, five_hour_end_ts)?;
        let weekly_window = Self::calculate_breakdown(&conn, account_key, weekly_start_ts, weekly_end_ts)?;
        let today = Self::calculate_breakdown(&conn, account_key, today_start_ts, i64::MAX)?;
        let current_month = Self::calculate_breakdown(&conn, account_key, month_start_ts, i64::MAX)?;
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
}
