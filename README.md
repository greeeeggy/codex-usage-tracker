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

Selecting an account in the header changes the history view. To change the actual Codex login, use **Switch** in the sidebar. Signing out clears the previous account's live display. The floating widget and local overlay always follow the signed-in account.

Meter identifies users and account/workspaces by a hashed combination of their IDs. Plan or email changes do not split an ID-backed account. Usage records contain account identity, email, and plan; credentials never enter Meter's database, UI, or API. Optional saved logins are held separately under Meter's local app-data folder, encrypted with Windows DPAPI for the current Windows user.

### Switch accounts

1. Open **Switch**, name your current login, and choose **Save current account**. Use **Add another account** to sign into each additional account once.
2. Choose **Enable live switching**. Quit Codex normally once (including its tray process), then reopen it from Start. Wait for **Desktop connected** in Meter.
3. Choose **Switch** beside a saved account. The running desktop engine adopts that login through its app-server connection. Meter compares its actual token and account response before reporting success. Codex stays open.

Setup installs a local launcher through the desktop's `CODEX_CLI_PATH` override and retains any previous override for **Disable live switching**. It does not modify the desktop installation or automatically close it. Replacing `auth.json` alone cannot prove that a running desktop has adopted the login; Meter also saves that file after the engine confirms the change, so future launches use the selected account.

Meter also backs up `CODEX_HOME/config.toml` and adds the native-auth `meter-live` provider definition without changing your default provider or other settings. This definition remains after disabling live switching because saved or cached chats can still refer to it. Version 0.5.1 repairs older setups on startup, including setups already disabled, to resolve **Model provider `meter-live` not found**. Retry or reopen an affected chat after the repair.

The bridge opts into the experimental `chatgptAuthTokens` login API, handles host-owned token renewal, and uses native ChatGPT HTTP streaming to avoid sockets retaining the outgoing login. Native OpenAI chat starts, resumes, and forks use that transport. On every bridge launch, Meter rediscovers the desktop engine even when the old cache still exists, prefers the running desktop's installed package, and refreshes its companion executables beside the bridge. This includes `codex-code-mode-host.exe`, which code-mode tools resolve beside `CODEX_CLI_PATH`. Meter startup also repairs missing companions in existing setups. It does not edit the Codex installation or automatically re-enable a removed launcher override. Updating Meter's bridge or a helper already in use needs one normal Codex quit/reopen. This is an experimental integration with desktop internals; unrelated future protocol changes may still require a Meter update.

Sign-in and renewal prefer the installed desktop engine over an older global npm CLI. If OpenAI rejects a saved token before its expiry, Meter attempts one renewal and retains rotated credentials even if desktop activation fails. A revoked refresh grant needs to be reconnected once through **Add another account**.

Finish or stop active work and voice before switching; Meter refuses busy connections. Failed confirmation attempts restore the engine's preceding login. Existing CLI/editor sessions are not restarted. Chats, workspace files, and recorded usage remain in place. File-based ChatGPT credentials are required; keyring-only and API-key logins are not supported. Revoked logins can be replaced through **Add another account**. Removing a saved login retains recorded usage.

### Usage Guard and AI connection

Open **Usage Guard** and choose **Enable Usage Guard**. Meter registers its executable as a read-only stdio MCP server and merges silent local command hooks for `SessionStart`, `UserPromptSubmit`, `PreToolUse`, and `Stop` into `CODEX_HOME/hooks.json`, preserving unrelated handlers and backing up the previous file. These hooks check Meter's already-monitored allowance without asking the AI to call a tool. Above the threshold they return no model context. Version 0.5.2 automatically migrates existing Meter hooks on startup. Restart Codex, then review/trust the changed hooks once through its hook settings (`/hooks` in the CLI). Use **Remove connection** to remove Meter's tool and hooks. For portable builds, keep the executable at the registered location; use **Update connection** if it moves.

At **5% or less** five-hour remaining, the hooks automatically deliver one prompt per chat and reset cycle telling the AI to reach a safe stopping point, save an evidence-based handoff, schedule **one continuation of the same chat at reset + two minutes**, and end its turn. The alert remains deduplicated across later turns, restarts, compaction, and simultaneous tool calls. `get_usage_guard` is available for an explicit usage question or a single fresh check before the scheduled continuation; the AI is instructed not to poll it during ordinary work. It must preserve previous instructions, avoid duplicate schedules, and state plainly when its client has no scheduling tool. An exhausted weekly allowance postpones the resume time to the later blocking reset.

The API never treats missing, stale, unverified, or already-reset usage as fresh available quota. Hooks avoid recursive Stop continuations and run at prompt/tool/turn boundaries; they cannot interrupt an already-running command or wake an idle chat. Meter supplies the prompt; the host AI schedules continuation using its supported scheduling tool. Closing Meter makes monitoring unavailable and the hooks stay silent. Local Codex/Work sessions support this path; hosted ChatGPT sessions need a separately reachable connector and do not automatically gain access to loopback services.

This design follows [OpenAI authentication](https://learn.chatgpt.com/docs/auth), [MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli), and [hook contracts](https://learn.chatgpt.com/docs/hooks). Live switching follows the app-server authentication contract and the [published desktop bridge approach](https://www.reddit.com/r/codex/comments/1waunnw/i_got_account_switching_working_in_codex_desktop/); implementation here is original. Ordinary file-swap tools require a desktop restart, as documented by [codex-auth](https://github.com/Loongphy/codex-auth).

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
| `/api/guard` | Freshness, pause decision, exact resume time, and checkpoint prompt |
| `/events` | Live updates through server-sent events |
| `/health` | Startup health check |

## Builds and verification

The [Windows GitHub Actions workflow](https://github.com/greeeeggy/codex-usage-tracker/actions/workflows/windows-build.yml) performs all application compilation on GitHub's Windows runners.

It checks frontend types, runs Rust regression tests for account isolation, migration, pricing, log replay, and limit windows, initializes the real Codex app-server without Command Prompt, and tests the dashboard in Chromium at compact and wide sizes. It then builds both Windows executables and verifies that startup with an older decorated window state still produces one title bar. The packaged bridge test reproduces a stale desktop login despite a changed auth file, verifies actual engine activation in both directions without a process restart, and checks busy/voice guards, recovery, private RPC isolation, renewal routing, and unknown-message forwarding using synthetic credentials. This test does not simulate live OpenAI inference or prove the whole desktop UI on a signed-in personal account. The downloadable artifact includes both executables and their build manifest.

The app uses Tauri 2, Rust, SQLite, React, TypeScript, Tailwind CSS, and Zustand.

## Version 0.5.2

Fixes code-mode tools missing `codex-code-mode-host.exe` after a desktop update: the live-switch bridge follows the current engine and refreshes matching runtime companions. Usage Guard now delivers automatic threshold alerts through silent local hooks, removes instructions to repeatedly call the guard tool, and deduplicates alerts across turns and concurrent hooks. Existing connections migrate on Meter startup. Includes packaged runtime-update and automatic-alert regression checks.

### Version 0.5.1

Fixes chats failing to resume after live switching is disabled or Codex starts through its normal launcher. Persists the compatibility provider before the bridge can create a chat, retains it when disabling, and repairs old enabled or disabled installations on startup. Includes a regression check that saves a synthetic chat using the packaged bridge's provider and resumes it with the official native engine after disabling, without credentials or inference.

### Version 0.5.0

Replaces the slow close/file-swap/relaunch path with a desktop app-server bridge. Switch results require confirmation from the running engine, including when its cached login differs from `auth.json`. Includes one-time setup, busy-work/voice guards, token renewal, recovery, and packaged Windows protocol tests. The 5% Usage Guard API and continuation prompt remain available.

### Version 0.4.0

Adds **Switch** with encrypted saved accounts, isolated first-time sign-in, normal Codex close/relaunch, verified activation and recovery. Adds **Usage Guard**, a local API, packaged MCP tool, and installable lifecycle hooks for the 5% handoff and reset-plus-two-minute continuation prompt. Windows builds remain on GitHub Actions, including bridge checks against the packaged executable.

### Version 0.3.1

Fixes duplicate window controls caused by restoring an old native title bar setting. The dashboard now has a neutral palette, smaller quota panels, flat token totals, working settings access, and a chart with an explicit height so observations remain visible. Account detection and separate usage/history remain available.

### Version 0.3.0

Automatic account detection and saved-account selection, separate account usage/history, recovery of identified local records, safe clearing on sign-out, and protection against delayed updates from a previous account. Includes the native Windows launch fix for error 740.
