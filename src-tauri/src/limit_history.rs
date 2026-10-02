use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitPeriod {
    pub id: i64,
    pub limit_id: String,
    pub limit_name: Option<String>,
    pub window_kind: String,
    pub started_at: i64,
    pub resets_at: i64,
    pub first_observed_at: i64,
    pub last_observed_at: i64,
    pub used_percent: f64,
    pub status: String,
    pub tokens: crate::db::TokenBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitHistoryPage {
    pub periods: Vec<LimitPeriod>,
    pub total: i64,
}

pub fn persist_snapshot(db: &crate::db::Db, snapshot: &crate::codex_client::UsageSnapshot) {
    let captured = chrono::DateTime::parse_from_rfc3339(&snapshot.captured_at)
        .map(|t| t.timestamp())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp());
    if snapshot.limits.is_empty() {
        persist_windows(
            db,
            snapshot.limit_id.as_deref().unwrap_or("codex"),
            snapshot.limit_name.as_deref(),
            &snapshot.windows,
            captured,
        );
    } else {
        for bucket in &snapshot.limits {
            persist_windows(
                db,
                &bucket.limit_id,
                bucket.limit_name.as_deref(),
                &bucket.windows,
                captured,
            );
        }
    }
}

fn persist_windows(
    db: &crate::db::Db,
    id: &str,
    name: Option<&str>,
    windows: &[crate::codex_client::UsageWindow],
    captured: i64,
) {
    for window in windows {
        let reset = window
            .resets_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
        if let (Some(reset), Some(duration)) = (reset, window.duration_minutes) {
            if let Err(error) = db.record_limit_period(
                id,
                name,
                &window.name,
                duration,
                reset.timestamp(),
                captured,
                window.used_percent,
            ) {
                log::warn!("Could not record limit period: {error}");
            }
        }
    }
}
