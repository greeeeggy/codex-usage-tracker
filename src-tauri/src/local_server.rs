use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Json,
    },
    routing::get,
    Router,
};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::usage_service::UsageState;

/// Shared state for the local HTTP server
#[derive(Clone)]
pub struct ServerState {
    pub usage_state: Arc<RwLock<UsageState>>,
    pub sse_tx: tokio::sync::broadcast::Sender<String>,
}

/// Start the local HTTP server on 127.0.0.1:32145
pub async fn start_server(
    usage_state: Arc<RwLock<UsageState>>,
    sse_tx: tokio::sync::broadcast::Sender<String>,
) {
    let state = ServerState {
        usage_state,
        sse_tx,
    };

    let app = Router::new()
        .route("/api/usage", get(api_usage))
        .route("/overlay", get(overlay_page))
        .route("/events", get(sse_events))
        .route("/health", get(health))
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind("127.0.0.1:32145").await {
        Ok(l) => l,
        Err(e) => {
            log::error!("Failed to bind local server on 127.0.0.1:32145: {}", e);
            return;
        }
    };

    log::info!("Local HTTP server running on http://127.0.0.1:32145");

    if let Err(e) = axum::serve(listener, app).await {
        log::error!("Local server error: {}", e);
    }
}

/// GET /api/usage — JSON response for Rainmeter / external consumers
async fn api_usage(State(state): State<ServerState>) -> impl IntoResponse {
    let usage = state.usage_state.read().await;

    if let Some(snapshot) = &usage.snapshot {
        let mut response = serde_json::Map::new();
        response.insert("account".into(), serde_json::json!(usage.active_account));

        for window in &snapshot.windows {
            let window_data = serde_json::json!({
                "remainingPercent": window.remaining_percent,
                "usedPercent": window.used_percent,
                "resetsAt": window.resets_at,
                "durationMinutes": window.duration_minutes,
            });

            response.insert(window.name.clone(), window_data);
        }

        response.insert(
            "planType".to_string(),
            serde_json::json!(snapshot.plan_type),
        );
        response.insert(
            "capturedAt".to_string(),
            serde_json::json!(snapshot.captured_at),
        );

        Json(serde_json::Value::Object(response)).into_response()
    } else {
        Json(serde_json::json!({
            "error": "No usage data available",
            "state": format!("{:?}", usage.monitor_state)
        }))
        .into_response()
    }
}

/// GET /overlay — transparent HTML page for OBS Browser Source
async fn overlay_page() -> Html<String> {
    Html(OBS_OVERLAY_HTML.to_string())
}

/// GET /events — Server-Sent Events for live updates
async fn sse_events(
    State(state): State<ServerState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.sse_tx.subscribe();
    let stream =
        BroadcastStream::new(rx).map(|msg| Ok(Event::default().data(msg.unwrap_or_default())));

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// GET /health — simple health check
async fn health() -> &'static str {
    "ok"
}

/// The embedded OBS overlay HTML
const OBS_OVERLAY_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Codex Meter - OBS Overlay</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body {
    background: transparent;
    font-family: 'Segoe UI', -apple-system, sans-serif;
    color: #fff;
    padding: 12px;
  }
  .container {
    background: rgba(15, 15, 20, 0.85);
    border-radius: 12px;
    padding: 14px 18px;
    backdrop-filter: blur(12px);
    border: 1px solid rgba(255, 255, 255, 0.08);
    max-width: 320px;
  }
  .header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1.5px;
    color: rgba(255, 255, 255, 0.5);
  }
  .header .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #22c55e;
    animation: pulse 2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
  .usage-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }
  .usage-label {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.6);
    width: 50px;
    flex-shrink: 0;
  }
  .bar-container {
    flex: 1;
    height: 8px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 4px;
    overflow: hidden;
  }
  .bar-fill {
    height: 100%;
    border-radius: 4px;
    transition: width 0.8s ease, background-color 0.5s ease;
  }
  .usage-pct {
    font-size: 12px;
    font-weight: 600;
    width: 38px;
    text-align: right;
    flex-shrink: 0;
  }
  .reset-info {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.4);
    margin-top: 4px;
  }
  .disconnected {
    text-align: center;
    color: rgba(255, 255, 255, 0.4);
    font-size: 12px;
    padding: 8px 0;
  }
</style>
</head>
<body>
<div class="container">
  <div class="header">
    <div class="dot" id="status-dot"></div>
    <span>CODEX</span>
  </div>
  <div id="usage-content">
    <div class="disconnected">Connecting...</div>
  </div>
  <div class="reset-info" id="reset-info"></div>
</div>
<script>
  const content = document.getElementById('usage-content');
  const resetInfo = document.getElementById('reset-info');
  const statusDot = document.getElementById('status-dot');

  function getBarColor(remaining) {
    if (remaining >= 50) return '#22c55e';
    if (remaining >= 25) return '#eab308';
    if (remaining >= 10) return '#f97316';
    return '#ef4444';
  }

  function formatCountdown(isoDate) {
    if (!isoDate) return '';
    const diff = new Date(isoDate).getTime() - Date.now();
    if (diff <= 0) return 'Resetting...';
    const h = Math.floor(diff / 3600000);
    const m = Math.floor((diff % 3600000) / 60000);
    if (h > 0) return h + 'h ' + m + 'm';
    return m + 'm';
  }

  function renderUsage(data) {
    statusDot.style.background = '#22c55e';
    let html = '';
    let resetText = '';

    const windows = [
      { key: 'fiveHour', label: '5h' },
      { key: 'weekly', label: 'Weekly' }
    ];

    for (const w of windows) {
      const d = data[w.key];
      if (!d) continue;
      const remaining = d.remainingPercent;
      const color = getBarColor(remaining);
      html += '<div class="usage-row">' +
        '<span class="usage-label">' + w.label + '</span>' +
        '<div class="bar-container"><div class="bar-fill" style="width:' +
        remaining + '%;background:' + color + '"></div></div>' +
        '<span class="usage-pct" style="color:' + color + '">' +
        Math.round(remaining) + '%</span></div>';

      if (d.resetsAt) {
        const cd = formatCountdown(d.resetsAt);
        if (cd) resetText += w.label + ' resets in ' + cd + '  ';
      }
    }

    content.innerHTML = html || '<div class="disconnected">No data</div>';
    resetInfo.textContent = resetText.trim();
  }

  // Connect to SSE
  const es = new EventSource('http://127.0.0.1:32145/events');
  es.onmessage = (e) => {
    try {
      const data = JSON.parse(e.data);
      renderUsage(data);
    } catch(err) {}
  };
  es.onerror = () => {
    statusDot.style.background = '#ef4444';
    content.innerHTML = '<div class="disconnected">Disconnected</div>';
  };

  // Also fetch initial data
  fetch('http://127.0.0.1:32145/api/usage')
    .then(r => r.json())
    .then(renderUsage)
    .catch(() => {});

  // Update countdown every second
  setInterval(() => {
    const infos = document.querySelectorAll('.reset-info');
    // Countdown is recalculated on each SSE update
  }, 1000);
</script>
</body>
</html>"#;
