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
