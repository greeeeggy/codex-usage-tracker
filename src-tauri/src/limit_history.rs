use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) struct LimitObservation {
    pub account_key: String,
    pub limit_id: String,
    pub limit_name: Option<String>,
    pub window_kind: String,
    pub duration_seconds: i64,
    pub resets_at: i64,
    pub captured_at: i64,
    pub used_percent: f64,
}

pub(crate) struct PeriodSummary {
    pub account_key: String,
    pub limit_id: String,
    pub limit_name: Option<String>,
    pub window_kind: String,
    pub started_at: i64,
    pub resets_at: i64,
    pub first_observed_at: i64,
    pub last_observed_at: i64,
    pub used_percent: f64,
}

/// Replay in observation order so importing older logs produces the same cycles
/// as monitoring them live. A changing estimate is not itself a quota reset.
pub(crate) fn summarize_observations(
    mut observations: Vec<LimitObservation>,
) -> Vec<PeriodSummary> {
    observations.sort_by(|a, b| (a.captured_at, a.resets_at).cmp(&(b.captured_at, b.resets_at)));
    let mut buckets: BTreeMap<(String, String, String), Vec<PeriodSummary>> = BTreeMap::new();
    for observation in observations {
        if observation.duration_seconds <= 0 || observation.resets_at <= observation.captured_at {
            // An expired response cannot start another active cycle.
            continue;
        }
        let key = (
            observation.account_key.clone(),
            observation.limit_id.clone(),
            observation.window_kind.clone(),
        );
        let periods = buckets.entry(key).or_default();
        let reported_start =
            (observation.resets_at - observation.duration_seconds).min(observation.captured_at);
        if let Some(period) = periods.last_mut() {
            if observation.captured_at < period.resets_at {
                // A start after all previous observations indicates an early
                // reset (e.g. an allowance refresh). Allow five minutes of
                // timestamp jitter; quota percentage changes alone are not resets.
                if reported_start > period.last_observed_at + 300 {
                    period.resets_at = reported_start;
                } else {
                    if observation.captured_at == period.last_observed_at {
                        period.used_percent = period.used_percent.max(observation.used_percent);
                    } else {
                        period.used_percent = observation.used_percent;
                    }
                    period.last_observed_at = observation.captured_at;
                    if observation.limit_name.is_some() {
                        period.limit_name = observation.limit_name;
                    }
                    continue;
                }
            }
        }
        // A newly reported interval can start slightly before the previous
        // reset. Keep token ranges disjoint within each bucket/window.
        let start = periods
            .last()
            .map_or(reported_start, |p| reported_start.max(p.resets_at));
        periods.push(PeriodSummary {
            account_key: observation.account_key,
            limit_id: observation.limit_id,
            limit_name: observation.limit_name,
            window_kind: observation.window_kind,
            started_at: start,
            resets_at: observation.resets_at,
            first_observed_at: observation.captured_at,
            last_observed_at: observation.captured_at,
            used_percent: observation.used_percent,
        });
    }
    buckets.into_values().flatten().collect()
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDay {
    pub start_date: String,
    pub tokens: i64,
    pub observed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDayPage {
    pub days: Vec<AccountDay>,
    pub total: i64,
}

pub fn persist_snapshot(db: &crate::db::Db, snapshot: &crate::codex_client::UsageSnapshot) {
    let account_key = snapshot
        .account_key
        .as_deref()
        .unwrap_or(crate::accounts::LEGACY_ACCOUNT);
    let _ = db.cache_snapshot(snapshot);
    let captured = chrono::DateTime::parse_from_rfc3339(&snapshot.captured_at)
        .map(|t| t.timestamp())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp());
    if snapshot.limits.is_empty() {
        persist_windows(
            db,
            account_key,
            snapshot.limit_id.as_deref().unwrap_or("codex"),
            snapshot.limit_name.as_deref(),
            &snapshot.windows,
            captured,
        );
    } else {
        for bucket in &snapshot.limits {
            persist_windows(
                db,
                account_key,
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
    account_key: &str,
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
            if let Err(error) = db.record_limit_period_for(
                account_key,
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
