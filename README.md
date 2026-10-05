# Codex Meter

A live, lightweight desktop monitor for the shared OpenAI Work/Codex usage allowance. Codex Meter runs in your system tray, records five-hour and weekly quota observations across desktop, CLI, IDE, web, cloud, and other clients using the same allowance, and alerts you when your quota is low.

## Features

- **Live Tracking**: Directly reads `codex app-server` data without API keys
- **Account Monitoring**: Keeps polling shared quota while the monitor is running, including when no local Codex client is open
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

## Pricing and limit history (0.2.1)

- Prices are fetched from the official OpenAI pricing Markdown every six hours, with hourly retry after failures. Model names and rates are discovered from the published standard text-token tables. No API key is required. Settings shows all discovered models, freshness, and a manual update button. A failed update retains the last successful SQLite cache; missing prices stay unavailable rather than falling back to another model.
- Cost estimates use standard API rates, including published cache-write and long-context rates. Subscription quota is separate. Session summary estimates use aggregate base rates; per-request details apply the long-context threshold. Pricing updates invalidate saved session summaries.
- Limits shows one summary per actual five-hour, weekly, or other reported cycle for every limit bucket: start, reset, total recorded tokens, and latest observed account-wide quota consumed. The token breakdown is available on hover. Reset estimates changing during an unexpired cycle update its observations instead of creating overlapping rows; its first observed boundary stays stable. The first valid observation at or after that reset starts a new cycle. A newly reported start more than five minutes after the previous observation detects an early allowance refresh and closes the previous cycle at that start; smaller timestamp jitter and quota percentage changes alone do not create cycles. Empty gaps are not invented. Completed cycles survive resets and restarts, and filters/pagination preserve older history.
- Upgrading automatically consolidates the duplicate period rows from 0.2.0. Original quota observations and request records remain saved. Summaries replay in timestamp order, so importing older logs or restarting yields the same cycles; token intervals within each bucket/window do not overlap.
- Rollout logs are replayed from durable byte checkpoints, including archived sessions. Token accounting uses original request timestamps and half-open intervals `[start, reset)`, so boundary requests enter the new period. Shared turn/counter identities deduplicate copied fork history and retain the earliest original timestamp. Repeated cumulative notifications are ignored, incomplete lines are retried, and token rows/checkpoints commit together. App-server token notifications are not added a second time.
- Shared quota monitoring includes Work/Codex, web/cloud, other devices, and connected clients consuming the same allowance. It does not depend on detecting a local window. Server-reported account lifetime/day token activity is fetched separately, cached, and retained by reported day; repeat observations replace each day's total instead of adding to it. Limits displays this activity separately from local request totals. A server lifetime summary below local usage, or zero while quota is used, is flagged as incomplete and cannot replace recorded usage with a misleading zero. Ordinary Chat conversations and separately billed API activity are not collected.
- Period token totals cover available local requests with shared-limit attribution. Rows lacking this attribution and old unscoped aggregates are retained but excluded from these totals. Exact remote per-period tokens, deleted logs, and observations missed while the monitor is closed cannot be reconstructed from the supported API. Daily account totals cannot be split reliably across arbitrary five-hour/weekly boundaries or model buckets. Five-hour and weekly views overlap; account and local totals must not be added together. Quota percentages are never converted into invented token totals. See the [official account usage and rate-limit API](https://learn.chatgpt.com/docs/app-server).

## GitHub Windows builds

The `Test and build Windows executable` workflow checks frontend types, runs Rust regression tests (including the live pricing source), checks the Limits/pricing screens in Chromium, builds a Windows executable and NSIS installer, and confirms the executable starts. The `Codex-Meter-Windows` artifact contains `Codex-Meter.exe`, `Codex-Meter-setup.exe`, and a build manifest with the source commit and SHA-256 hashes. This workflow performs all application compilation on GitHub Actions.
