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
    #[serde(default)]
    pub account_key: Option<String>,
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
    active_account: Mutex<String>,
}

impl Db {
    #[cfg(test)]
    pub(crate) fn test_db(path: Option<&std::path::Path>) -> Self {
        let conn = path
            .map(|p| Connection::open(p).unwrap())
            .unwrap_or_else(|| Connection::open_in_memory().unwrap());
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            active_account: Mutex::new(crate::accounts::LEGACY_ACCOUNT.into()),
        };
        db.initialize_schema().unwrap();
        db
    }
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
            active_account: Mutex::new(crate::accounts::LEGACY_ACCOUNT.into()),
        };

        db.initialize_schema()?;

        Ok(db)
    }

    fn initialize_schema(&self) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();

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

            CREATE TABLE IF NOT EXISTS limit_period_history (
                id INTEGER PRIMARY KEY,
                account_key TEXT NOT NULL DEFAULT 'default',
                limit_id TEXT NOT NULL,
                limit_name TEXT,
                window_kind TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                resets_at INTEGER NOT NULL,
                first_observed_at INTEGER NOT NULL,
                last_observed_at INTEGER NOT NULL,
                used_percent REAL NOT NULL,
                UNIQUE(account_key, limit_id, window_kind, resets_at)
            );
            CREATE TABLE IF NOT EXISTS limit_observations (
                account_key TEXT NOT NULL,
                limit_id TEXT NOT NULL,
                limit_name TEXT,
                window_kind TEXT NOT NULL,
                duration_seconds INTEGER NOT NULL,
                resets_at INTEGER NOT NULL,
                captured_at INTEGER NOT NULL,
                used_percent REAL NOT NULL,
                PRIMARY KEY(account_key, limit_id, window_kind, captured_at, resets_at)
            );
            CREATE TABLE IF NOT EXISTS window_token_events (
                id TEXT PRIMARY KEY,
                account_key TEXT NOT NULL,
                limit_id TEXT NOT NULL,
                captured_at INTEGER NOT NULL,
                thread_id TEXT NOT NULL,
                input_tokens INTEGER NOT NULL,
                cached_input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                reasoning_tokens INTEGER,
                total_tokens INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_window_tokens_range ON window_token_events(account_key, limit_id, captured_at);
            CREATE TABLE IF NOT EXISTS rollout_checkpoints (
                file_key TEXT PRIMARY KEY,
                offset INTEGER NOT NULL,
                state_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS account_usage_days (
                start_date TEXT PRIMARY KEY,
                tokens INTEGER NOT NULL,
                observed_at INTEGER NOT NULL
            );
            DROP VIEW IF EXISTS counted_token_events;
            CREATE VIEW counted_token_events AS
                SELECT account_key, captured_at, thread_id, input_tokens, cached_input_tokens,
                       output_tokens, reasoning_tokens, total_tokens FROM window_token_events
                WHERE limit_id <> 'unattributed';
            "
        ).map_err(|e| format!("Failed to initialize schema: {}", e))?;

        // Retain the original observations before replacing duplicate summary
        // rows. Migration and its marker commit together and are restart-safe.
        let migrated = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM settings WHERE key = 'limit_summary_migrated_v1')",
                [],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?;
        if !migrated {
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            tx.execute_batch("INSERT OR IGNORE INTO limit_observations
                SELECT account_key, 'codex', NULL, window_kind, duration_minutes * 60, resets_at, captured_at, used_percent
                FROM quota_samples WHERE duration_minutes > 0 AND duration_minutes <= 525600 AND resets_at > 0;
                INSERT OR IGNORE INTO limit_observations
                SELECT account_key, limit_id, limit_name, window_kind, resets_at - started_at, resets_at, first_observed_at, 0
                FROM limit_period_history;
                INSERT OR REPLACE INTO limit_observations
                SELECT account_key, limit_id, limit_name, window_kind, resets_at - started_at, resets_at, last_observed_at, used_percent
                FROM limit_period_history;
                INSERT OR REPLACE INTO settings VALUES ('limit_summary_migrated_v1', 'true', 0);
                INSERT OR REPLACE INTO settings VALUES ('limit_summary_dirty', 'true', 0);")
                .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
        }
        let has_column = |table: &str, column: &str| -> Result<bool, String> {
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(|e| e.to_string())?;
            let names = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            let found = names.filter_map(Result::ok).any(|name| name == column);
            Ok(found)
        };
        let migrate_days = !has_column("account_usage_days", "account_key")?;
        let migrate_events = !has_column("app_events", "account_key")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        if migrate_days {
            tx.execute_batch("ALTER TABLE account_usage_days RENAME TO account_usage_days_legacy;
                CREATE TABLE account_usage_days (account_key TEXT NOT NULL, start_date TEXT NOT NULL, tokens INTEGER NOT NULL, observed_at INTEGER NOT NULL, PRIMARY KEY(account_key, start_date));
                INSERT INTO account_usage_days SELECT 'default',start_date,tokens,observed_at FROM account_usage_days_legacy;
                DROP TABLE account_usage_days_legacy;").map_err(|e| e.to_string())?;
        }
        if migrate_events {
            tx.execute_batch(
                "ALTER TABLE app_events ADD COLUMN account_key TEXT NOT NULL DEFAULT 'default';",
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute_batch("CREATE INDEX IF NOT EXISTS idx_quota_account_kind ON quota_samples(account_key,window_kind,captured_at);
            CREATE INDEX IF NOT EXISTS idx_app_events_account ON app_events(account_key,captured_at);").map_err(|e| e.to_string())?;
        tx.execute("INSERT OR IGNORE INTO settings SELECT 'account_usage_cache:default',value_json,updated_at FROM settings WHERE key = 'account_usage_cache'", []).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_active_account(&self, key: &str) {
        *self.active_account.lock().unwrap() = key.into();
    }

    pub fn remember_account(
        &self,
        profile: &crate::accounts::AccountProfile,
    ) -> Result<(), String> {
        let key = format!("account_profile:{}", profile.account_key);
        let existing = self
            .get_setting(&key)?
            .and_then(|json| serde_json::from_str::<crate::accounts::AccountProfile>(&json).ok());
        let mut profile = profile.clone();
        if let Some(existing) = existing {
            if profile.email.is_none() {
                profile.email = existing.email;
                profile.label = existing.label;
            }
            if profile.plan_type.is_none() {
                profile.plan_type = existing.plan_type;
            }
        }
        self.set_setting(
            &key,
            &serde_json::to_string(&profile).map_err(|e| e.to_string())?,
        )?;
        let now = chrono::Utc::now().timestamp();
        self.conn.lock().unwrap().execute("INSERT INTO accounts VALUES (?1,?2,?3,?3) ON CONFLICT(account_key) DO UPDATE SET plan_type=COALESCE(excluded.plan_type,plan_type),last_seen_at=excluded.last_seen_at", params![profile.account_key,profile.plan_type,now]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<crate::accounts::AccountProfile>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT value_json FROM settings WHERE key LIKE 'account_profile:%'")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut profiles: Vec<_> = rows
            .filter_map(Result::ok)
            .filter_map(|s| serde_json::from_str::<crate::accounts::AccountProfile>(&s).ok())
            .collect();
        profiles.sort_by(|a, b| {
            a.label
                .cmp(&b.label)
                .then(a.account_key.cmp(&b.account_key))
        });
        profiles.push(crate::accounts::AccountProfile::legacy());
        Ok(profiles)
    }

    pub fn cache_snapshot(
        &self,
        snapshot: &crate::codex_client::UsageSnapshot,
    ) -> Result<(), String> {
        let Some(key) = &snapshot.account_key else {
            return Ok(());
        };
        let captured = chrono::DateTime::parse_from_rfc3339(&snapshot.captured_at)
            .map(|t| t.timestamp())
            .map_err(|e| e.to_string())?;
        self.conn.lock().unwrap().execute("INSERT INTO settings VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at WHERE excluded.updated_at>=settings.updated_at", params![format!("quota_cache:{key}"),serde_json::to_string(snapshot).map_err(|e| e.to_string())?,captured]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn cached_snapshot(
        &self,
        account_key: &str,
    ) -> Result<Option<crate::codex_client::UsageSnapshot>, String> {
        Ok(self
            .get_setting(&format!("quota_cache:{account_key}"))?
            .and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub fn cached_account_usage(
        &self,
        account_key: &str,
    ) -> Result<Option<crate::codex_client::AccountUsage>, String> {
        Ok(self
            .get_setting(&format!("account_usage_cache:{account_key}"))?
            .and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub fn assign_rollout_account(&self, file_key: &str, account_key: &str) -> Result<(), String> {
        if account_key == crate::accounts::LEGACY_ACCOUNT {
            return Ok(());
        }
        self.conn.lock().unwrap().execute("UPDATE window_token_events SET account_key=?1 WHERE thread_id=?2 AND account_key='default'",params![account_key,file_key]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        use rusqlite::OptionalExtension;
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn set_setting(&self, key: &str, json: &str) -> Result<(), String> {
        self.conn.lock().unwrap().execute("INSERT INTO settings VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            params![key, json, chrono::Utc::now().timestamp()]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn record_account_usage(
        &self,
        usage: &crate::codex_client::AccountUsage,
    ) -> Result<(), String> {
        let captured = usage
            .fetched_at
            .unwrap_or_else(|| chrono::Utc::now().timestamp());
        let json = serde_json::to_string(usage).map_err(|e| e.to_string())?;
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let account_key = usage
            .account_key
            .as_deref()
            .unwrap_or(crate::accounts::LEGACY_ACCOUNT);
        for day in usage.daily_usage_buckets.as_deref().unwrap_or(&[]) {
            if day.tokens < 0
                || chrono::NaiveDate::parse_from_str(&day.start_date, "%Y-%m-%d").is_err()
            {
                continue;
            }
            // Server buckets are totals, not increments. Repeated polls replace
            // the observation; they must never add local usage a second time.
            tx.execute("INSERT INTO account_usage_days VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(account_key,start_date) DO UPDATE SET tokens = excluded.tokens, observed_at = excluded.observed_at
                WHERE excluded.observed_at >= observed_at", params![account_key, day.start_date, day.tokens, captured])
                .map_err(|e| e.to_string())?;
        }
        tx.execute("INSERT INTO settings VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at
            WHERE excluded.updated_at >= updated_at", params![format!("account_usage_cache:{account_key}"),json, captured]).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    #[cfg(test)]
    pub fn get_account_usage_days(
        &self,
        offset: i64,
    ) -> Result<crate::limit_history::AccountDayPage, String> {
        self.get_account_usage_days_for("default", offset)
    }

    pub fn get_account_usage_days_for(
        &self,
        account_key: &str,
        offset: i64,
    ) -> Result<crate::limit_history::AccountDayPage, String> {
        let conn = self.conn.lock().unwrap();
        let total = conn
            .query_row(
                "SELECT COUNT(*) FROM account_usage_days WHERE account_key=?1",
                [account_key],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT start_date, tokens, observed_at FROM account_usage_days WHERE account_key=?1 ORDER BY start_date DESC LIMIT 100 OFFSET ?2")
            .map_err(|e| e.to_string())?;
        let days = stmt
            .query_map(params![account_key, offset.max(0)], |r| {
                Ok(crate::limit_history::AccountDay {
                    start_date: r.get(0)?,
                    tokens: r.get(1)?,
                    observed_at: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        Ok(crate::limit_history::AccountDayPage { days, total })
    }

    #[cfg(test)]
    pub fn record_limit_period(
        &self,
        limit_id: &str,
        limit_name: Option<&str>,
        kind: &str,
        minutes: u64,
        reset: i64,
        captured: i64,
        used: f64,
    ) -> Result<(), String> {
        self.record_limit_period_for(
            "default", limit_id, limit_name, kind, minutes, reset, captured, used,
        )
    }

    pub fn record_limit_period_for(
        &self,
        account_key: &str,
        limit_id: &str,
        limit_name: Option<&str>,
        kind: &str,
        minutes: u64,
        reset: i64,
        captured: i64,
        used: f64,
    ) -> Result<(), String> {
        if minutes == 0 || minutes > 525_600 || reset <= 0 || !used.is_finite() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let changed = tx.execute("INSERT INTO limit_observations
            (account_key, limit_id, limit_name, window_kind, duration_seconds, resets_at, captured_at, used_percent)
            VALUES (?8, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(account_key, limit_id, window_kind, captured_at, resets_at) DO UPDATE SET
                limit_name = COALESCE(excluded.limit_name, limit_name), used_percent = excluded.used_percent
            WHERE used_percent <> excluded.used_percent OR (excluded.limit_name IS NOT NULL AND limit_name IS NOT excluded.limit_name)",
            params![limit_id, limit_name, kind, minutes as i64 * 60, reset, captured, used.clamp(0.0, 100.0),account_key])
            .map_err(|e| e.to_string())?;
        if changed > 0 {
            tx.execute(
                "INSERT OR REPLACE INTO settings VALUES ('limit_summary_dirty', 'true', 0)",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }

    fn refresh_limit_summaries(conn: &mut Connection) -> Result<(), String> {
        let dirty = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM settings WHERE key = 'limit_summary_dirty')",
                [],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?;
        if !dirty {
            return Ok(());
        }
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let observations = {
            let mut stmt = tx.prepare("SELECT account_key, limit_id, limit_name, window_kind, duration_seconds, resets_at, captured_at, used_percent FROM limit_observations")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(crate::limit_history::LimitObservation {
                        account_key: r.get(0)?,
                        limit_id: r.get(1)?,
                        limit_name: r.get(2)?,
                        window_kind: r.get(3)?,
                        duration_seconds: r.get(4)?,
                        resets_at: r.get(5)?,
                        captured_at: r.get(6)?,
                        used_percent: r.get(7)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?
        };
        tx.execute("DELETE FROM limit_period_history", [])
            .map_err(|e| e.to_string())?;
        for period in crate::limit_history::summarize_observations(observations) {
            tx.execute("INSERT INTO limit_period_history
                (account_key, limit_id, limit_name, window_kind, started_at, resets_at, first_observed_at, last_observed_at, used_percent)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![period.account_key, period.limit_id, period.limit_name, period.window_kind, period.started_at,
                    period.resets_at, period.first_observed_at, period.last_observed_at, period.used_percent])
                .map_err(|e| e.to_string())?;
        }
        tx.execute("DELETE FROM settings WHERE key = 'limit_summary_dirty'", [])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    pub fn backfill_limit_periods(&self) -> Result<(), String> {
        if self.get_setting("limit_history_backfilled")?.is_some() {
            return Ok(());
        }
        // Existing quota observations preserve known reset boundaries. Never
        // manufacture periods in unobserved gaps.
        let samples: Vec<(String, i64, String, i64, i64, f64)> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT account_key, captured_at, window_kind, duration_minutes, resets_at, used_percent
                FROM quota_samples WHERE resets_at IS NOT NULL AND duration_minutes > 0 ORDER BY captured_at")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?
        };
        for (account, captured, kind, duration, reset, used) in samples {
            self.record_limit_period_for(
                &account,
                "codex",
                None,
                &kind,
                duration as u64,
                reset,
                captured,
                used,
            )?;
        }
        self.set_setting("limit_history_backfilled", "true")?;
        Ok(())
    }

    #[cfg(test)]
    pub fn get_limit_history(
        &self,
        limit_id: Option<&str>,
        kind: Option<&str>,
        offset: i64,
        now: i64,
    ) -> Result<crate::limit_history::LimitHistoryPage, String> {
        self.get_limit_history_for("default", limit_id, kind, offset, now)
    }

    pub fn get_limit_history_for(
        &self,
        account_key: &str,
        limit_id: Option<&str>,
        kind: Option<&str>,
        offset: i64,
        now: i64,
    ) -> Result<crate::limit_history::LimitHistoryPage, String> {
        let mut conn = self.conn.lock().unwrap();
        Self::refresh_limit_summaries(&mut conn)?;
        let total = conn
            .query_row(
                "SELECT COUNT(*) FROM limit_period_history WHERE
            (?1 IS NULL OR limit_id = ?1) AND (?2 IS NULL OR window_kind = ?2) AND account_key=?3",
                params![limit_id, kind, account_key],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, limit_id, limit_name, window_kind, started_at, resets_at,
            first_observed_at, last_observed_at, used_percent FROM limit_period_history WHERE
            (?1 IS NULL OR limit_id = ?1) AND (?2 IS NULL OR window_kind = ?2) AND account_key=?4
            ORDER BY resets_at DESC, limit_id, window_kind LIMIT 100 OFFSET ?3",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit_id, kind, offset.max(0), account_key], |r| {
                let start: i64 = r.get(4)?;
                let reset: i64 = r.get(5)?;
                Ok(crate::limit_history::LimitPeriod {
                    id: r.get(0)?,
                    limit_id: r.get(1)?,
                    limit_name: r.get(2)?,
                    window_kind: r.get(3)?,
                    started_at: start,
                    resets_at: reset,
                    first_observed_at: r.get(6)?,
                    last_observed_at: r.get(7)?,
                    used_percent: r.get(8)?,
                    status: if reset <= now { "completed" } else { "active" }.into(),
                    tokens: TokenBreakdown::default(),
                })
            })
            .map_err(|e| e.to_string())?;
        let mut periods = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        for period in &mut periods {
            period.tokens = conn.query_row("SELECT COALESCE(SUM(input_tokens),0), COALESCE(SUM(cached_input_tokens),0),
                COALESCE(SUM(output_tokens),0), COALESCE(SUM(reasoning_tokens),0), COALESCE(SUM(total_tokens),0)
                FROM window_token_events WHERE account_key = ?4 AND limit_id = ?1 AND captured_at >= ?2 AND captured_at < ?3",
                params![period.limit_id, period.started_at, period.resets_at,account_key], |r| {
                    let input: i64 = r.get(0)?;
                    let cached: i64 = r.get(1)?;
                    Ok(TokenBreakdown { input_tokens: input, cached_input_tokens: cached, uncached_input_tokens: (input - cached).max(0),
                        output_tokens: r.get(2)?, reasoning_tokens: Some(r.get(3)?), total_tokens: r.get(4)? })
                }).map_err(|e| e.to_string())?;
        }
        Ok(crate::limit_history::LimitHistoryPage { periods, total })
    }

    pub fn get_rollout_checkpoint(&self, key: &str) -> Result<Option<(u64, String)>, String> {
        use rusqlite::OptionalExtension;
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT offset, state_json FROM rollout_checkpoints WHERE file_key = ?1",
                [key],
                |r| Ok((r.get::<_, i64>(0)?.max(0) as u64, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn save_rollout_batch(
        &self,
        key: &str,
        offset: u64,
        state_json: &str,
        events: &[(TokenEvent, String)],
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for (event, limit_id) in events {
            tx.execute(
                "INSERT INTO window_token_events VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(id) DO UPDATE SET
                    account_key = CASE WHEN window_token_events.account_key='default' THEN excluded.account_key ELSE window_token_events.account_key END,
                    limit_id = excluded.limit_id, thread_id = excluded.thread_id,
                    captured_at = MIN(excluded.captured_at,window_token_events.captured_at), input_tokens = excluded.input_tokens,
                    cached_input_tokens = excluded.cached_input_tokens, output_tokens = excluded.output_tokens,
                    reasoning_tokens = excluded.reasoning_tokens, total_tokens = excluded.total_tokens
                WHERE excluded.captured_at < window_token_events.captured_at OR (window_token_events.account_key='default' AND excluded.account_key<>'default')",
                params![
                    event.id,
                    event.account_key,
                    limit_id,
                    event.captured_at,
                    event.thread_id,
                    event.input_tokens,
                    event.cached_input_tokens,
                    event.output_tokens,
                    event.reasoning_tokens,
                    event.total_tokens
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute(
            "INSERT INTO rollout_checkpoints VALUES (?1, ?2, ?3) ON CONFLICT(file_key)
            DO UPDATE SET offset = excluded.offset, state_json = excluded.state_json",
            params![key, offset as i64, state_json],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
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
        account_key: &str,
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
                 WHERE window_kind = ?1 AND captured_at >= ?2 AND account_key=?4
                 ORDER BY captured_at DESC
                 LIMIT ?3
             )
             ORDER BY captured_at ASC",
            )
            .map_err(|e| format!("Prepare failed: {}", e))?;

        let rows = stmt
            .query_map(params![window_kind, since_ts, limit, account_key], |row| {
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
        account_key: &str,
        window_kind: &str,
        session_start_ts: i64,
    ) -> Result<UsageDeltas, String> {
        let conn = self.conn.lock().unwrap();
        let now = Local::now();

        let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let today_start_ts = Local.from_local_datetime(&today_start).unwrap().timestamp();
        let one_hour_ago = chrono::Utc::now().timestamp() - 3600;

        // Session delta: difference between first and latest sample since session start
        let session_delta = Self::calc_delta(&conn, account_key, window_kind, session_start_ts)?;

        // Today delta: difference between first and latest sample since midnight
        let today_delta = Self::calc_delta(&conn, account_key, window_kind, today_start_ts)?;

        // Peak hour: max used_percent in the last hour
        let peak_hour_used: f64 = conn.query_row(
            "SELECT COALESCE(MAX(used_percent), 0.0) FROM quota_samples WHERE window_kind = ?1 AND captured_at >= ?2 AND account_key=?3",
            params![window_kind, one_hour_ago,account_key],
            |row| row.get(0),
        ).unwrap_or(0.0);

        // Sessions today: count distinct monitoring sessions by counting app_events of type session_started
        let sessions_today: i64 = conn.query_row(
            "SELECT COUNT(*) FROM app_events WHERE event_type = 'session_started' AND captured_at >= ?1 AND account_key=?2",
            params![today_start_ts,account_key],
            |row| row.get(0),
        ).unwrap_or(0);

        // Longest session: find the longest gap between session_started and session_ended today
        let longest_session_minutes =
            Self::calc_longest_session(&conn, account_key, today_start_ts)?;

        Ok(UsageDeltas {
            session_delta,
            today_delta,
            peak_hour_used,
            sessions_today,
            longest_session_minutes,
        })
    }

    fn calc_delta(
        conn: &Connection,
        account_key: &str,
        window_kind: &str,
        since_ts: i64,
    ) -> Result<f64, String> {
        let first: Option<f64> = conn.query_row(
            "SELECT used_percent FROM quota_samples WHERE window_kind = ?1 AND captured_at >= ?2 AND account_key=?3 ORDER BY captured_at ASC LIMIT 1",
            params![window_kind, since_ts,account_key],
            |row| row.get(0),
        ).ok();

        let latest: Option<f64> = conn.query_row(
            "SELECT used_percent FROM quota_samples WHERE window_kind = ?1 AND captured_at >= ?2 AND account_key=?3 ORDER BY captured_at DESC LIMIT 1",
            params![window_kind, since_ts,account_key],
            |row| row.get(0),
        ).ok();

        match (first, latest) {
            (Some(f), Some(l)) => Ok((l - f).max(0.0)),
            _ => Ok(0.0),
        }
    }

    fn calc_longest_session(
        conn: &Connection,
        account_key: &str,
        since_ts: i64,
    ) -> Result<i64, String> {
        // Get all session start/end events today, ordered by time
        let mut stmt = conn
            .prepare(
                "SELECT event_type, captured_at FROM app_events
             WHERE event_type IN ('session_started', 'session_ended') AND captured_at >= ?1 AND account_key=?2
             ORDER BY captured_at ASC",
            )
            .map_err(|e| format!("Prepare failed: {}", e))?;

        let events: Vec<(String, i64)> = stmt
            .query_map(params![since_ts, account_key], |row| {
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
            "INSERT INTO app_events (event_type, label, captured_at, description,account_key) VALUES (?1, ?2, ?3, ?4,?5)",
            params![event_type, label, now, description,self.active_account.lock().unwrap().clone()],
        ).map_err(|e| format!("Failed to insert app event: {}", e))?;
        Ok(())
    }

    /// Get recent app events, newest first. Returns at most `limit` events.
    pub fn get_recent_events(
        &self,
        account_key: &str,
        limit: i64,
    ) -> Result<Vec<AppEvent>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT event_type, label, captured_at, description
             FROM app_events WHERE account_key=?2
             ORDER BY captured_at DESC
             LIMIT ?1",
            )
            .map_err(|e| format!("Prepare failed: {}", e))?;

        let rows = stmt
            .query_map(params![limit, account_key], |row| {
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
             FROM counted_token_events
             WHERE account_key = ?1 AND captured_at >= ?2 AND captured_at < ?3",
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
                     FROM counted_token_events
                     WHERE account_key = ?1
                       AND thread_id IS NOT NULL
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
                 FROM counted_token_events
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
            account_key: Some(account_key.into()),
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
             FROM counted_token_events
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

#[cfg(test)]
mod limit_tests {
    use super::*;

    fn event(id: &str, timestamp: i64, total: i64) -> TokenEvent {
        TokenEvent {
            id: id.into(),
            account_key: "default".into(),
            captured_at: timestamp,
            client_type: "local_session".into(),
            thread_id: Some("test-rollout".into()),
            turn_id: None,
            project_path_hash: None,
            model: None,
            input_tokens: total - 10,
            cached_input_tokens: 20,
            output_tokens: 10,
            reasoning_tokens: Some(5),
            total_tokens: total,
            event_type: "local_rollout".into(),
        }
    }

    #[test]
    fn archives_five_hour_periods_and_assigns_boundary_to_new_period() {
        let db = Db::test_db(None);
        let reset = 1_800_000_000;
        db.record_limit_period("codex", None, "fiveHour", 300, reset, reset - 60, 80.0)
            .unwrap();
        db.record_limit_period(
            "codex",
            None,
            "fiveHour",
            300,
            reset + 18_000,
            reset + 60,
            5.0,
        )
        .unwrap();
        db.save_rollout_batch(
            "test-rollout",
            1,
            "{}",
            &[
                (event("one", reset - 1, 100), "codex".into()),
                (event("two", reset, 50), "codex".into()),
            ],
        )
        .unwrap();
        let page = db
            .get_limit_history(None, Some("fiveHour"), 0, reset + 60)
            .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.periods[0].tokens.total_tokens, 50);
        assert_eq!(page.periods[1].tokens.total_tokens, 100);
        assert_eq!(page.periods[1].status, "completed");
        assert_eq!(page.periods[0].status, "active");
    }

    #[test]
    fn weekly_reset_and_multiple_buckets_remain_independent() {
        let db = Db::test_db(None);
        let reset = 1_800_000_000;
        for bucket in ["codex", "other-model-limit"] {
            db.record_limit_period(bucket, None, "weekly", 10080, reset, reset - 10, 50.0)
                .unwrap();
            db.record_limit_period(
                bucket,
                None,
                "weekly",
                10080,
                reset + 604800,
                reset + 10,
                1.0,
            )
            .unwrap();
        }
        db.save_rollout_batch(
            "test-rollout",
            1,
            "{}",
            &[
                (event("one", reset - 1, 100), "codex".into()),
                (event("two", reset - 1, 50), "other-model-limit".into()),
                (event("three", reset, 70), "codex".into()),
            ],
        )
        .unwrap();
        let codex = db
            .get_limit_history(Some("codex"), Some("weekly"), 0, reset + 1)
            .unwrap();
        assert_eq!(codex.total, 2);
        assert_eq!(codex.periods[0].tokens.total_tokens, 70);
        assert_eq!(codex.periods[1].tokens.total_tokens, 100);
        let other = db
            .get_limit_history(Some("other-model-limit"), None, 0, reset + 1)
            .unwrap();
        assert_eq!(other.periods[1].tokens.total_tokens, 50);
    }

    #[test]
    fn stale_replay_does_not_replace_last_quota_observation() {
        let db = Db::test_db(None);
        let reset = 1_800_000_000;
        db.record_limit_period("codex", None, "weekly", 10080, reset, reset - 5, 80.0)
            .unwrap();
        db.record_limit_period("codex", None, "weekly", 10080, reset, reset - 60, 20.0)
            .unwrap();
        let page = db.get_limit_history(None, None, 0, reset).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.periods[0].used_percent, 80.0);
        assert_eq!(page.periods[0].first_observed_at, reset - 60);
    }

    #[test]
    fn restart_keeps_history_and_replay_does_not_double_count() {
        let path = std::env::temp_dir().join(format!("meter-test-{}.db", uuid::Uuid::new_v4()));
        let reset = 1_800_000_000;
        let rows = vec![(event("one", reset - 1, 100), "codex".into())];
        {
            let db = Db::test_db(Some(&path));
            db.record_limit_period("codex", None, "fiveHour", 300, reset, reset - 10, 30.0)
                .unwrap();
            db.save_rollout_batch("test-rollout", 500, "{}", &rows)
                .unwrap();
        }
        {
            let db = Db::test_db(Some(&path));
            db.save_rollout_batch("test-rollout", 500, "{}", &rows)
                .unwrap();
            assert_eq!(
                db.get_rollout_checkpoint("test-rollout")
                    .unwrap()
                    .unwrap()
                    .0,
                500
            );
            let page = db.get_limit_history(None, None, 0, reset + 1).unwrap();
            assert_eq!(page.periods[0].tokens.total_tokens, 100);
            assert_eq!(page.periods[0].status, "completed");
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn old_schema_samples_backfill_without_duplicate_periods() {
        let db = Db::test_db(None);
        db.insert_quota_sample(
            "default",
            1_799_999_000,
            "fiveHour",
            Some(300),
            20.0,
            80.0,
            Some(1_800_000_000),
            "primary",
        )
        .unwrap();
        db.backfill_limit_periods().unwrap();
        db.backfill_limit_periods().unwrap();
        assert_eq!(
            db.get_limit_history(None, None, 0, 1_800_000_001)
                .unwrap()
                .total,
            1
        );
    }

    #[test]
    fn zero_usage_placeholder_does_not_create_a_period_per_poll() {
        let db = Db::test_db(None);
        let reset = 1_800_000_000;
        db.record_limit_period("codex", None, "fiveHour", 300, reset, reset - 60, 0.0)
            .unwrap();
        db.record_limit_period("codex", None, "fiveHour", 300, reset + 30, reset - 30, 0.0)
            .unwrap();
        assert_eq!(
            db.get_limit_history(None, None, 0, reset - 30)
                .unwrap()
                .total,
            1
        );
        db.record_limit_period(
            "codex",
            None,
            "fiveHour",
            300,
            reset + 18_000,
            reset + 1,
            0.0,
        )
        .unwrap();
        assert_eq!(
            db.get_limit_history(None, None, 0, reset + 1)
                .unwrap()
                .total,
            2
        );
    }

    #[test]
    fn moving_nonzero_reset_estimates_summarize_one_cycle_per_window() {
        let db = Db::test_db(None);
        let start = 1_800_000_000;
        for (kind, minutes) in [("fiveHour", 300), ("weekly", 10080)] {
            let reset = start + minutes * 60;
            for (delta, used) in [(0, 10.0), (1, 12.0), (20, 68.0), (120, 65.0)] {
                db.record_limit_period(
                    "codex",
                    None,
                    kind,
                    minutes as u64,
                    reset + delta,
                    start + 10 + delta,
                    used,
                )
                .unwrap();
            }
            let page = db
                .get_limit_history(None, Some(kind), 0, start + 1000)
                .unwrap();
            assert_eq!(page.total, 1);
            assert_eq!(page.periods[0].started_at, start);
            assert_eq!(page.periods[0].resets_at, reset);
            assert_eq!(page.periods[0].used_percent, 65.0);
            assert_eq!(page.periods[0].last_observed_at, start + 130);
        }
        db.save_rollout_batch(
            "tokens",
            1,
            "{}",
            &[
                (event("first", start + 10, 100), "codex".into()),
                (event("second", start + 130, 50), "codex".into()),
            ],
        )
        .unwrap();
        let page = db.get_limit_history(None, None, 0, start + 1000).unwrap();
        assert_eq!(page.total, 2);
        assert!(page.periods.iter().all(|p| p.tokens.total_tokens == 150));
    }

    #[test]
    fn shifted_weekly_estimates_do_not_spawn_overlapping_active_cycles() {
        let db = Db::test_db(None);
        let start = 1_800_000_000;
        let week = 604800;
        // Corrections whose inferred starts precede the previous observation
        // belong to that cycle, even when the correction spans several days.
        for day in [0, 1, 3, 4] {
            db.record_limit_period(
                "codex",
                None,
                "weekly",
                10080,
                start + week + day * 86400,
                start + 5 * 86400 + 10 + day,
                20.0 + day as f64,
            )
            .unwrap();
        }
        let page = db
            .get_limit_history(None, None, 0, start + 5 * 86400)
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.periods[0].resets_at, start + week);
        db.record_limit_period(
            "codex",
            None,
            "weekly",
            10080,
            start + 2 * week,
            start + week,
            1.0,
        )
        .unwrap();
        let page = db.get_limit_history(None, None, 0, start + week).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(
            page.periods.iter().filter(|p| p.status == "active").count(),
            1
        );
    }

    #[test]
    fn early_reset_closes_previous_cycle_instead_of_leaving_two_active() {
        let db = Db::test_db(None);
        let start = 1_800_000_000;
        let new_start = start + 2 * 86400;
        db.record_limit_period(
            "codex",
            None,
            "weekly",
            10080,
            start + 604800,
            start + 10,
            89.0,
        )
        .unwrap();
        db.record_limit_period(
            "codex",
            None,
            "weekly",
            10080,
            new_start + 604800,
            new_start + 10,
            4.0,
        )
        .unwrap();
        db.save_rollout_batch(
            "early-reset",
            1,
            "{}",
            &[
                (event("before-refresh", new_start - 1, 400), "codex".into()),
                (event("after-refresh", new_start, 21), "codex".into()),
            ],
        )
        .unwrap();
        let page = db.get_limit_history(None, None, 0, new_start + 10).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.periods[1].resets_at, new_start);
        assert_eq!(page.periods[1].used_percent, 89.0);
        assert_eq!(page.periods[1].status, "completed");
        assert_eq!(page.periods[1].tokens.total_tokens, 400);
        assert_eq!(page.periods[0].started_at, new_start);
        assert_eq!(page.periods[0].status, "active");
        assert_eq!(page.periods[0].tokens.total_tokens, 21);
    }

    #[test]
    fn late_replay_rebuilds_cycles_and_expired_responses_do_not_create_cycles() {
        let db = Db::test_db(None);
        let start = 1_800_000_000;
        let reset = start + 18000;
        db.record_limit_period(
            "codex",
            None,
            "fiveHour",
            300,
            reset + 100,
            start + 200,
            60.0,
        )
        .unwrap();
        db.get_limit_history(None, None, 0, start + 200).unwrap();
        db.record_limit_period("codex", None, "fiveHour", 300, reset, start + 10, 10.0)
            .unwrap();
        db.record_limit_period(
            "codex",
            None,
            "fiveHour",
            300,
            reset + 17990,
            reset + 1,
            2.0,
        )
        .unwrap();
        db.record_limit_period("codex", None, "fiveHour", 300, reset, reset + 2, 100.0)
            .unwrap();
        db.save_rollout_batch(
            "boundary",
            1,
            "{}",
            &[
                (event("before", reset - 1, 100), "codex".into()),
                (event("at", reset, 50), "codex".into()),
            ],
        )
        .unwrap();
        let page = db.get_limit_history(None, None, 0, reset + 2).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.periods[0].started_at, reset);
        assert_eq!(page.periods[0].tokens.total_tokens, 50);
        assert_eq!(page.periods[1].resets_at, reset);
        assert_eq!(page.periods[1].tokens.total_tokens, 100);
        assert_eq!(page.periods[1].used_percent, 60.0);
    }

    #[test]
    fn upgrade_repairs_existing_duplicate_history_and_keeps_completed_cycles() {
        let path =
            std::env::temp_dir().join(format!("meter-migration-{}.db", uuid::Uuid::new_v4()));
        let start = 1_800_000_000;
        let week = 604800;
        {
            let db = Db::test_db(Some(&path));
            {
                let conn = db.conn.lock().unwrap();
                conn.execute(
                    "DELETE FROM settings WHERE key = 'limit_summary_migrated_v1'",
                    [],
                )
                .unwrap();
                for (delta, captured, used) in [
                    (0, 10, 20.0),
                    (1, 20, 21.0),
                    (30, 50, 25.0),
                    (60, 3 * 86400 + 10, 89.0),
                    (week, week + 10, 5.0),
                ] {
                    conn.execute("INSERT INTO limit_period_history
                        (limit_id, window_kind, started_at, resets_at, first_observed_at, last_observed_at, used_percent)
                        VALUES ('codex', 'weekly', ?1, ?2, ?3, ?3, ?4)",
                        params![start + delta, start + week + delta, start + captured, used]).unwrap();
                }
            }
            db.save_rollout_batch(
                "migration",
                1,
                "{}",
                &[
                    (event("old", start + 50, 400_000_000), "codex".into()),
                    (event("new", start + week, 21_000_000), "codex".into()),
                ],
            )
            .unwrap();
        }
        for _ in 0..2 {
            let db = Db::test_db(Some(&path));
            let page = db
                .get_limit_history(None, None, 0, start + week + 20)
                .unwrap();
            assert_eq!(page.total, 2);
            assert_eq!(page.periods[0].tokens.total_tokens, 21_000_000);
            assert_eq!(page.periods[1].tokens.total_tokens, 400_000_000);
            assert_eq!(page.periods[1].used_percent, 89.0);
            assert_eq!(page.periods[1].status, "completed");
            assert_eq!(page.periods[1].started_at, start);
            assert_eq!(page.periods[1].resets_at, start + week);
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn unscoped_legacy_totals_are_retained_but_not_treated_as_shared_limit_usage() {
        let db = Db::test_db(None);
        let legacy = event("old", 1_799_999_000, 100);
        db.insert_token_event(&legacy).unwrap();
        assert_eq!(
            db.get_token_totals("default", 0, 0, i64::MAX, 0, i64::MAX)
                .unwrap()
                .all_time_recorded
                .total_tokens,
            0
        );
        db.save_rollout_batch(
            "test-rollout",
            500,
            "{}",
            &[(event("exact", legacy.captured_at, 100), "codex".into())],
        )
        .unwrap();
        assert_eq!(
            db.get_token_totals("default", 0, 0, i64::MAX, 0, i64::MAX)
                .unwrap()
                .all_time_recorded
                .total_tokens,
            100
        );
        assert_eq!(
            db.conn
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM token_events", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn unassigned_requests_do_not_enter_shared_limit_token_totals() {
        let db = Db::test_db(None);
        db.save_rollout_batch(
            "unknown",
            10,
            "{}",
            &[(event("unknown", 1_799_999_000, 999), "unattributed".into())],
        )
        .unwrap();
        assert_eq!(
            db.get_token_totals("default", 0, 0, i64::MAX, 0, i64::MAX)
                .unwrap()
                .all_time_recorded
                .total_tokens,
            0
        );
    }

    #[test]
    fn server_day_totals_replace_observations_and_survive_missing_buckets() {
        let db = Db::test_db(None);
        let usage = |captured, tokens| crate::codex_client::AccountUsage {
            account_key: None,
            summary: None,
            fetched_at: Some(captured),
            daily_usage_buckets: Some(vec![crate::codex_client::DailyUsageBucket {
                start_date: "2026-10-02".into(),
                tokens,
            }]),
        };
        db.record_account_usage(&usage(100, 200)).unwrap();
        db.record_account_usage(&usage(101, 250)).unwrap();
        db.record_account_usage(&usage(101, 250)).unwrap();
        db.record_account_usage(&usage(99, 50)).unwrap();
        db.record_account_usage(&crate::codex_client::AccountUsage {
            fetched_at: Some(102),
            ..Default::default()
        })
        .unwrap();
        let page = db.get_account_usage_days(0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.days[0].tokens, 250);
        assert_eq!(page.days[0].observed_at, 101);
        // Account-wide totals already include local activity; they are kept
        // separately and are never added to request-level window totals.
        assert_eq!(
            db.get_token_totals("default", 0, 0, i64::MAX, 0, i64::MAX)
                .unwrap()
                .all_time_recorded
                .total_tokens,
            0
        );
    }

    #[test]
    fn separate_accounts_keep_tokens_periods_days_charts_and_events_after_restart() {
        let path =
            std::env::temp_dir().join(format!("meter-accounts-{}.sqlite", uuid::Uuid::new_v4()));
        let now = chrono::Utc::now().timestamp();
        let a = crate::accounts::identity("workspace-a", "user");
        let b = crate::accounts::identity("workspace-b", "user");
        {
            let db = Db::test_db(Some(&path));
            for (profile, tokens, percent) in [(&a, 100, 10.0), (&b, 900, 80.0)] {
                db.remember_account(profile).unwrap();
                let mut request = event(&profile.account_key, now - 10, tokens);
                request.account_key = profile.account_key.clone();
                db.save_rollout_batch(&profile.account_key, 1, "{}", &[(request, "codex".into())])
                    .unwrap();
                db.record_limit_period_for(
                    &profile.account_key,
                    "codex",
                    None,
                    "fiveHour",
                    300,
                    now + 60,
                    now,
                    percent,
                )
                .unwrap();
                db.record_account_usage(&crate::codex_client::AccountUsage {
                    account_key: Some(profile.account_key.clone()),
                    fetched_at: Some(now),
                    summary: None,
                    daily_usage_buckets: Some(vec![crate::codex_client::DailyUsageBucket {
                        start_date: "2026-10-05".into(),
                        tokens,
                    }]),
                })
                .unwrap();
                db.insert_quota_sample(
                    &profile.account_key,
                    now,
                    "weekly",
                    Some(10080),
                    percent,
                    100.0 - percent,
                    Some(now + 60),
                    "primary",
                )
                .unwrap();
                db.set_active_account(&profile.account_key);
                db.insert_app_event("session_started", &profile.account_key, None)
                    .unwrap();
            }
        }
        {
            let db = Db::test_db(Some(&path));
            for (profile, tokens, percent) in [(&a, 100, 10.0), (&b, 900, 80.0)] {
                assert_eq!(
                    db.get_token_totals(&profile.account_key, 0, 0, i64::MAX, 0, i64::MAX)
                        .unwrap()
                        .all_time_recorded
                        .total_tokens,
                    tokens
                );
                let periods = db
                    .get_limit_history_for(&profile.account_key, None, None, 0, now)
                    .unwrap();
                assert_eq!(periods.total, 1);
                assert_eq!(periods.periods[0].tokens.total_tokens, tokens);
                assert_eq!(periods.periods[0].used_percent, percent);
                assert_eq!(
                    db.get_account_usage_days_for(&profile.account_key, 0)
                        .unwrap()
                        .days[0]
                        .tokens,
                    tokens
                );
                assert_eq!(
                    db.cached_account_usage(&profile.account_key)
                        .unwrap()
                        .unwrap()
                        .account_key,
                    Some(profile.account_key.clone())
                );
                assert_eq!(
                    db.get_recent_quota_samples(&profile.account_key, "weekly", 0, 20)
                        .unwrap()[0]
                        .used_percent,
                    percent
                );
                assert_eq!(
                    db.get_usage_deltas(&profile.account_key, "weekly", 0)
                        .unwrap()
                        .peak_hour_used,
                    percent
                );
                assert_eq!(
                    db.get_recent_events(&profile.account_key, 20).unwrap()[0].label,
                    profile.account_key
                );
            }
            assert_eq!(
                db.get_limit_history_for("default", None, None, 0, now)
                    .unwrap()
                    .total,
                0
            );
            assert_eq!(db.list_accounts().unwrap().len(), 3);
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn migrates_old_combined_days_cache_and_events_to_unassigned_history() {
        let path =
            std::env::temp_dir().join(format!("meter-legacy-{}.sqlite", uuid::Uuid::new_v4()));
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE account_usage_days(start_date TEXT PRIMARY KEY,tokens INTEGER NOT NULL,observed_at INTEGER NOT NULL);
                INSERT INTO account_usage_days VALUES('2026-10-02',123,99);
                CREATE TABLE app_events(id INTEGER PRIMARY KEY AUTOINCREMENT,event_type TEXT NOT NULL,label TEXT NOT NULL,captured_at INTEGER NOT NULL,description TEXT);
                INSERT INTO app_events VALUES(7,'session_started','Old session',99,NULL);
                CREATE TABLE settings(key TEXT PRIMARY KEY,value_json TEXT NOT NULL,updated_at INTEGER NOT NULL);
                INSERT INTO settings VALUES('account_usage_cache','{\"summary\":null,\"dailyUsageBuckets\":null,\"fetchedAt\":99}',99);").unwrap();
        }
        for _ in 0..2 {
            let db = Db::test_db(Some(&path));
            assert_eq!(
                db.get_account_usage_days_for("default", 0).unwrap().days[0].tokens,
                123
            );
            assert_eq!(
                db.get_account_usage_days_for("new-account", 0)
                    .unwrap()
                    .total,
                0
            );
            assert_eq!(
                db.get_recent_events("default", 20).unwrap()[0].label,
                "Old session"
            );
            assert!(db.get_recent_events("new-account", 20).unwrap().is_empty());
            assert!(db.cached_account_usage("default").unwrap().is_some());
            assert!(db.cached_account_usage("new-account").unwrap().is_none());
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn backfill_keeps_the_quota_samples_original_account() {
        let db = Db::test_db(None);
        for (key, used) in [("a", 10.0), ("b", 80.0)] {
            db.insert_quota_sample(
                key,
                1799999900,
                "fiveHour",
                Some(300),
                used,
                100.0 - used,
                Some(1800000000),
                "primary",
            )
            .unwrap();
        }
        db.backfill_limit_periods().unwrap();
        for (key, used) in [("a", 10.0), ("b", 80.0)] {
            let history = db
                .get_limit_history_for(key, None, None, 0, 1800000001)
                .unwrap();
            assert_eq!(history.total, 1);
            assert_eq!(history.periods[0].used_percent, used);
        }
        assert_eq!(
            db.get_limit_history_for("default", None, None, 0, 1800000001)
                .unwrap()
                .total,
            0
        );
    }
}
