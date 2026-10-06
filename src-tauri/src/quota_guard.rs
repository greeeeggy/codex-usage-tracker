use crate::usage_service::{MonitorState, UsageState};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

pub const THRESHOLD: f64 = 5.0;
pub fn evaluate(state: &UsageState, now: DateTime<Utc>) -> Value {
    let unavailable = |reason: &str| json!({"status":"unavailable", "shouldPause":false, "thresholdPercent":THRESHOLD, "reason":reason, "prompt":null, "resumeAt":null});
    let Some(snapshot) = &state.snapshot else {
        return unavailable("No live usage data; do not assume quota is available.");
    };
    if state.monitor_state != MonitorState::Monitoring
        || state
            .active_account
            .as_ref()
            .map(|p| p.account_key.as_str())
            != snapshot.account_key.as_deref()
        || state.active_account.is_none()
    {
        return unavailable("Account is reconnecting or usage ownership is unverified.");
    }
    let Some(captured) = DateTime::parse_from_rfc3339(&snapshot.captured_at).ok() else {
        return unavailable("Usage timestamp is invalid.");
    };
    let age = (now - captured.with_timezone(&Utc)).num_seconds();
    if !(-5..=90).contains(&age) {
        return unavailable("Usage data is stale. Refresh before making a quota decision.");
    }
    let windows: Vec<_> = snapshot
        .limits
        .iter()
        .flat_map(|b| b.windows.iter())
        .chain(snapshot.windows.iter())
        .filter(|w| w.duration_minutes == Some(300) || w.name == "fiveHour")
        .collect();
    if windows.is_empty() {
        return unavailable("The account did not report a five-hour limit.");
    }
    if windows
        .iter()
        .any(|w| !w.remaining_percent.is_finite() || !(0.0..=100.0).contains(&w.remaining_percent))
    {
        return unavailable("Invalid five-hour quota percentage.");
    }
    let remaining = windows
        .iter()
        .map(|w| w.remaining_percent)
        .fold(100.0_f64, f64::min);
    let low: Vec<_> = windows
        .iter()
        .filter(|w| w.remaining_percent <= THRESHOLD)
        .collect();
    if low.is_empty() {
        return json!({"status":"ready", "shouldPause":false, "thresholdPercent":THRESHOLD, "remainingPercent":remaining, "capturedAt":snapshot.captured_at, "accountKey":snapshot.account_key, "prompt":null, "resumeAt":null});
    }
    let mut reset = now;
    for window in &low {
        let Some(time) = window
            .resets_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc))
        else {
            return unavailable("Five-hour quota is low but its reset time is missing. Do not invent a resume time.");
        };
        if time <= now {
            return unavailable("The reported reset has passed. Wait for refreshed usage.");
        }
        reset = reset.max(time);
    }
    // A depleted weekly allowance can prevent continuation even after the five-hour reset.
    for window in snapshot
        .limits
        .iter()
        .flat_map(|b| b.windows.iter())
        .chain(snapshot.windows.iter())
        .filter(|w| {
            (w.duration_minutes == Some(10080) || w.name == "weekly") && w.remaining_percent <= 0.0
        })
    {
        let Some(time) = window
            .resets_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc))
        else {
            return unavailable("The weekly allowance is exhausted and its reset time is unknown.");
        };
        reset = reset.max(time);
    }
    let resume = reset + Duration::minutes(2);
    let key = snapshot.account_key.as_deref().unwrap_or_default();
    let prompt = format!("Codex Meter quota checkpoint: you have {remaining:.1}% remaining in the five-hour allowance (pause threshold: 5%). Stop starting new substantive work and reach a safe stopping point now. Save pending edits and preserve a concise handoff in this chat and, when appropriate, the workspace: the user's objective and constraints; completed changes and file locations; checks actually run and their results; unfinished work and blockers; and the exact next action. Do not claim unfinished work or unrun checks succeeded. The blocking allowance resets at {}. Schedule ONE continuation of THIS SAME CHAT at {} (reset plus exactly two minutes). Its message must say: 'Continue the original task from the saved handoff. First check Codex Meter usage again; resume only if fresh usage is above 5% and no applicable limit is exhausted. Preserve all previous instructions and unrelated changes.' Use the app's supported scheduling tool and an exact timestamp, not a recurring poll or an estimated delay. Reuse or update an existing quota-resume schedule for this chat instead of creating duplicates. If scheduling is unavailable, report that plainly with the exact resume time; do not pretend a continuation was scheduled. After the handoff and scheduling, end the turn and wait. Do not switch accounts automatically.", reset.to_rfc3339(), resume.to_rfc3339());
    json!({"status":"pause", "shouldPause":true, "thresholdPercent":THRESHOLD, "remainingPercent":remaining, "capturedAt":snapshot.captured_at, "accountKey":key, "checkpointId":format!("{key}:{}", reset.timestamp()), "resetsAt":reset.to_rfc3339(), "resumeAt":resume.to_rfc3339(), "prompt":prompt})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accounts,
        codex_client::{LimitBucket, UsageSnapshot, UsageWindow},
    };
    fn state(remaining: f64) -> (UsageState, DateTime<Utc>) {
        let now = DateTime::parse_from_rfc3339("2026-10-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let account = accounts::identity("account", "user");
        let w = UsageWindow {
            source: "primary".into(),
            name: "fiveHour".into(),
            duration_minutes: Some(300),
            used_percent: 100.0 - remaining,
            remaining_percent: remaining,
            resets_at: Some("2026-10-06T11:00:00Z".into()),
        };
        let snapshot = UsageSnapshot {
            account_key: Some(account.account_key.clone()),
            captured_at: now.to_rfc3339(),
            limit_id: None,
            limit_name: None,
            plan_type: None,
            rate_limit_reached_type: None,
            credits: None,
            windows: vec![w.clone()],
            limits: vec![LimitBucket {
                limit_id: "codex".into(),
                limit_name: None,
                windows: vec![w],
            }],
            latest_context_window: None,
            latest_context_load_percent: None,
            latest_last_request_tokens: None,
        };
        (
            UsageState {
                active_account: Some(account),
                snapshot: Some(snapshot),
                monitor_state: MonitorState::Monitoring,
                ..Default::default()
            },
            now,
        )
    }
    #[test]
    fn inclusive_threshold_and_exact_two_minute_buffer() {
        for remaining in [5.0, 4.9, 0.0] {
            let (s, now) = state(remaining);
            let g = evaluate(&s, now);
            assert_eq!(g["shouldPause"], true);
            assert_eq!(g["resumeAt"], "2026-10-06T11:02:00+00:00");
        }
        let (s, n) = state(5.1);
        assert_eq!(evaluate(&s, n)["status"], "ready");
    }
    #[test]
    fn stale_signed_out_and_expired_reset_are_unavailable() {
        let (mut s, n) = state(5.0);
        assert_eq!(
            evaluate(&s, n + Duration::seconds(91))["status"],
            "unavailable"
        );
        s.active_account = None;
        assert_eq!(evaluate(&s, n)["status"], "unavailable");
        let (mut s, n) = state(5.0);
        s.snapshot.as_mut().unwrap().limits.clear();
        s.snapshot.as_mut().unwrap().windows[0].resets_at = Some(n.to_rfc3339());
        assert_eq!(evaluate(&s, n)["status"], "unavailable");
    }
    #[test]
    fn exhausted_weekly_and_multiple_buckets_use_latest_blocking_reset() {
        let (mut s, n) = state(5.0);
        let mut w = s.snapshot.as_ref().unwrap().windows[0].clone();
        w.name = "weekly".into();
        w.duration_minutes = Some(10080);
        w.remaining_percent = 0.0;
        w.resets_at = Some("2026-10-08T11:00:00Z".into());
        s.snapshot.as_mut().unwrap().windows.push(w);
        assert_eq!(evaluate(&s, n)["resumeAt"], "2026-10-08T11:02:00+00:00");
    }
    #[test]
    fn missing_reset_and_invalid_percentage_do_not_guess() {
        let (mut s, n) = state(5.0);
        s.snapshot.as_mut().unwrap().limits[0].windows[0].resets_at = None;
        assert_eq!(evaluate(&s, n)["status"], "unavailable");
        let (mut s, n) = state(5.0);
        s.snapshot.as_mut().unwrap().limits[0].windows[0].remaining_percent = f64::NAN;
        assert_eq!(evaluate(&s, n)["status"], "unavailable");
    }
}
