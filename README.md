# Codex Meter

A Windows desktop monitor for your ChatGPT account's shared Work/Codex allowance. See five-hour and weekly quota, local token usage, saved limit periods, and estimated API-equivalent costs from a dashboard, floating widget, or system tray.

## Download and install

Get the latest Windows build from [GitHub Releases](https://github.com/greeeeggy/codex-usage-tracker/releases/latest).

- **Codex-Meter-setup.exe** installs Meter and its shortcuts.
- **Codex-Meter.exe** runs without an installer.
- **build-info.json** identifies the source commit and includes SHA-256 hashes for both executables.

Meter requires the official Codex CLI and a ChatGPT sign-in:

```powershell
npm install -g @openai/codex
codex login
```

Start Meter after signing in. On Windows, Meter launches the native Codex executable directly, including installations made through npm. This fixes Windows error 740 when Command Prompt is configured to run as administrator; changing that Windows compatibility setting is unnecessary.

The dashboard has one title bar with minimize, maximize/restore, and close controls. Closing keeps Meter running in the tray. Saved window size and position are restored; old settings cannot enable a second Windows title bar.

The overview uses compact quota bars, a single token totals row, and a dated quota chart. Explanations of local/server coverage and cost estimates are available under **About these counts and estimates**.

## Accounts

Meter detects the signed-in account and follows Codex sign-in changes automatically. Each account/workspace has separate quotas, recorded tokens, daily server totals, charts, chats, and history.

The **Account** menu offers:

- **Signed-in account**: follows the account currently signed into the CLI's configured `CODEX_HOME`, with fresh quotas.
- **A saved account**: shows that account's stored records and the time they were last observed. Sign into it in Codex to refresh its quotas.
- **Earlier history (unassigned)**: retains older records whose owner cannot be established.

Selecting an account in Meter does not log into it or change Codex credentials. Signing out clears the previous account's live display. The floating widget and local overlay always follow the signed-in account.

Meter identifies users and account/workspaces by a hashed combination of their IDs. Plan or email changes do not split an ID-backed account. It reads only identity information from local credentials and saves the account key, display email, and plan; tokens and passwords are never stored in Meter's database or sent to its UI.

Newer local session headers include ownership IDs, allowing Meter to recover the matching account's history. Older combined records stay unassigned rather than being guessed to belong to the current account. Keychain installations exposing only an email use a separate email-based identity; an email alone cannot establish workspace ownership.

## Usage and history

- Shared quotas are read from `codex app-server`, including allowance consumed through other devices and clients. Polling continues while Meter runs, even with no local Codex window open.
- Five-hour and weekly limit cycles remain saved after resets and restarts. Limits shows observed quota consumption and locally recorded tokens for each cycle and limit bucket.
- Server lifetime and daily token activity are cached separately. Repeated observations replace a day's reported total instead of adding it again. Incomplete server summaries are flagged and cannot replace a larger known local lifetime total.
- Local chats show token breakdowns, cache rate, request details, and API-equivalent cost estimates when a matching price is available.
- Low-quota desktop alerts fire at 75%, 50%, 25%, and 0% remaining. The dashboard and always-on-top widget show live quota and reset times.

Five-hour and weekly token views overlap. Server account totals already include local activity, so these totals must not be added together. Exact remote token usage per limit period, deleted logs, and quota observations missed while Meter is closed cannot be reconstructed. Ordinary Chat conversations and separately billed API activity are not collected.

Local accounting uses original request timestamps and half-open intervals `[start, reset)`. Durable checkpoints resume after restart, incomplete log lines are retried, and copied fork requests are counted once. Requests without shared-limit attribution are retained separately and excluded from limit token totals. Quota percentages are never converted into invented token counts.

## Pricing

Meter fetches standard text-token pricing from [OpenAI's official pricing source](https://developers.openai.com/api/docs/pricing.md) every six hours, with retry after failures. Settings displays discovered model rates, their freshness, and a manual update button.

Estimates include published cached-input, cache-write, and long-context rates where available. Missing model prices remain unavailable, and a failed refresh retains the last successful SQLite cache. These are API-equivalent estimates, not subscription charges.

See the [official app-server account and rate-limit documentation](https://learn.chatgpt.com/docs/app-server) for the account data used by Meter.

## OBS and Rainmeter

Meter's local server listens on `127.0.0.1:32145`:

| Address | Purpose |
| --- | --- |
| `/overlay` | Transparent OBS browser-source overlay |
| `/api/usage` | Signed-in account identity and live quota JSON |
| `/events` | Live updates through server-sent events |
| `/health` | Startup health check |

## Builds and verification

The [Windows GitHub Actions workflow](https://github.com/greeeeggy/codex-usage-tracker/actions/workflows/windows-build.yml) performs all application compilation on GitHub's Windows runners.

It checks frontend types, runs Rust regression tests for account isolation, migration, pricing, log replay, and limit windows, initializes the real Codex app-server without Command Prompt, and tests the dashboard in Chromium at compact and wide sizes. It then builds both Windows executables and verifies that startup with an older decorated window state still produces one title bar. The downloadable artifact includes both executables and their build manifest.

The app uses Tauri 2, Rust, SQLite, React, TypeScript, Tailwind CSS, and Zustand.

## Version 0.3.1

Fixes duplicate window controls caused by restoring an old native title bar setting. The dashboard now has a neutral palette, smaller quota panels, flat token totals, working settings access, and a chart with an explicit height so observations remain visible. Account detection and separate usage/history remain available.

### Version 0.3.0

Automatic account detection and saved-account selection, separate account usage/history, recovery of identified local records, safe clearing on sign-out, and protection against delayed updates from a previous account. Includes the native Windows launch fix for error 740.
