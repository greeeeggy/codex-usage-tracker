# Codex Meter

A live, lightweight desktop monitor for OpenAI Codex usage limits. Codex Meter runs silently in your system tray, tracks your 5-hour and weekly usage across all Codex clients (Desktop App, CLI, VS Code), and alerts you when your quota is low.

## Features

- **Live Tracking**: Directly reads `codex app-server` data without API keys
- **Smart Activation**: Runs in dormant mode (near-zero CPU) until a Codex client opens
- **Floating Widget**: Always-on-top compact desktop widget
- **System Tray**: Quick access to usage data and settings
- **Desktop Alerts**: Notifications at 25%, 10%, and 5% remaining
- **OBS Integration**: Built-in HTTP server provides a transparent overlay (`http://127.0.0.1:32145/overlay`)
- **Rainmeter Ready**: JSON API available at `http://127.0.0.1:32145/api/usage`

## Prerequisites

Codex Meter depends on the official OpenAI Codex CLI to read your ChatGPT-authenticated usage quota.

```bash
# Install the official Codex CLI
npm install -g @openai/codex

# Authenticate with your ChatGPT account
codex login
```

## Running locally

```bash
# Install dependencies
npm install

# Run the Tauri development app
npm run tauri dev
```

## Architecture

Codex Meter is built with:
- **Tauri 2**: Lightweight Rust desktop framework
- **React + TypeScript**: Frontend UI
- **Tailwind CSS v4**: Styling
- **Zustand**: State management

## Pricing and limit history (0.2.0)

- Prices are fetched from the official OpenAI pricing Markdown every six hours, with hourly retry after failures. Model names and rates are discovered from the published standard text-token tables. No API key is required. Settings shows all discovered models, freshness, and a manual update button. A failed update retains the last successful SQLite cache; missing prices stay unavailable rather than falling back to another model.
- Cost estimates use standard API rates, including published cache-write and long-context rates. Subscription quota is separate. Session summary estimates use aggregate base rates; per-request details apply the long-context threshold. Pricing updates invalidate saved session summaries.
- Limits shows persistent five-hour, weekly, and other reported windows for every limit bucket. Each record has start/reset timestamps, current or completed status, recorded token breakdown, and last observed quota percentage. Records survive resets and restarts and can be filtered and paged without deleting older periods.
- Rollout logs are replayed from durable byte checkpoints, including archived sessions. Token accounting uses original request timestamps and half-open intervals `[start, reset)`, so boundary requests enter the new period. Shared turn/counter identities deduplicate copied fork history and retain the earliest original timestamp. Repeated cumulative notifications are ignored, incomplete lines are retried, and token rows/checkpoints commit together. App-server token notifications are not added a second time.
- Totals represent logs available on this computer. Other-device usage, deleted logs, and missing limit attribution cannot be reconstructed. Five-hour and weekly views overlap. The quota API reports percentages rather than exact token caps; quota percentages are not converted into token totals.

## GitHub Windows builds

The `Test and build Windows executable` workflow checks frontend types, runs Rust regression tests (including the live pricing source), checks the Limits/pricing screens in Chromium, builds a Windows executable and NSIS installer, and confirms the executable starts. The `Codex-Meter-Windows` artifact contains `Codex-Meter.exe`, `Codex-Meter-setup.exe`, and a build manifest with the source commit and SHA-256 hashes. This workflow performs all application compilation on GitHub Actions.
