import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const callbacks = new Map<number, (event: unknown) => void>();
    const listeners = new Map<string, number[]>();
    let id = 0;
    const tokens = { inputTokens: 90, cachedInputTokens: 30, uncachedInputTokens: 60, outputTokens: 10, reasoningTokens: 5, totalTokens: 100 };
    const reset = Math.floor(Date.now() / 1000) + 3600;
    const periods = [
      { id: 1, limitId: 'codex', limitName: null, windowKind: 'fiveHour', startedAt: reset - 18000, resetsAt: reset, firstObservedAt: reset - 17000, lastObservedAt: reset - 30, usedPercent: 20, status: 'active', tokens },
      { id: 2, limitId: 'codex', limitName: null, windowKind: 'weekly', startedAt: reset - 604800, resetsAt: reset, firstObservedAt: reset - 600000, lastObservedAt: reset - 30, usedPercent: 40, status: 'active', tokens },
      { id: 3, limitId: 'codex', limitName: null, windowKind: 'fiveHour', startedAt: reset - 36000, resetsAt: reset - 18000, firstObservedAt: reset - 35000, lastObservedAt: reset - 18001, usedPercent: 85, status: 'completed', tokens },
      { id: 4, limitId: 'new-model-limit', limitName: 'New model', windowKind: 'weekly', startedAt: reset - 604800, resetsAt: reset, firstObservedAt: reset - 600000, lastObservedAt: reset - 30, usedPercent: 30, status: 'active', tokens: { ...tokens, totalTokens: 250 } },
    ];
    const catalog = { source: 'https://developers.openai.com/api/docs/pricing.md', fetchedAt: Math.floor(Date.now() / 1000), lastError: null,
      models: { 'future-model': { standard: { input: 2, cachedInput: 0.1, cacheWrite: 2.5, output: 10 }, longContext: null, longContextThreshold: null } } };
    const window = { source: 'primary', name: 'fiveHour', durationMinutes: 300, usedPercent: 20, remainingPercent: 80, resetsAt: new Date(reset * 1000).toISOString() };
    const emit = (name: string, payload: unknown) => { for (const callback of listeners.get(name) ?? []) callbacks.get(callback)?.({ event: name, id: callback, payload }); };
    const profiles = [
      { accountKey: 'account-a', label: 'a@example.test', email: 'a@example.test', planType: 'plus', isLegacy: false },
      { accountKey: 'account-b', label: 'b@example.test', email: 'b@example.test', planType: 'pro', isLegacy: false },
      { accountKey: 'default', label: 'Earlier history (unassigned)', email: null, planType: null, isLegacy: true },
    ];
    let active: string | null = 'account-a';
    let maximized = false;
    const windowCommands: string[] = [];
    let savedLogins = profiles.slice(0, 2).map((account, index) => ({ id: `saved-${index}`, label: index ? 'Work' : 'Personal', account, savedAt: new Date().toISOString() }));
    let loginPending = false;
    let guardInstalled = false;
    let liveEnabled = true;
    Object.assign(globalThis, {
      __meterEmit: emit,
      __meterSignIn: (key: string | null) => { active = key; emit('accounts-updated', { activeAccount: profiles.find(p => p.accountKey === key) ?? null, accounts: profiles }); emit('state-changed', key ? 'monitoring' : 'authRequired'); },
      __meterDelayAccount: null,
      __meterWindowCommands: windowCommands,
    });
    Object.assign(globalThis, { __TAURI_INTERNALS__: {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
      transformCallback: (callback: (event: unknown) => void) => { callbacks.set(++id, callback); return id; },
      unregisterCallback: (callback: number) => callbacks.delete(callback),
      invoke: async (command: string, args: Record<string, unknown> = {}) => {
        if (command === 'plugin:event|listen') { const list = listeners.get(args.event as string) ?? []; list.push(args.handler as number); listeners.set(args.event as string, list); return ++id; }
        if (command === 'plugin:window|is_maximized') return maximized;
        if (command.startsWith('plugin:window|')) {
          windowCommands.push(command);
          if (command === 'plugin:window|toggle_maximize') { maximized = !maximized; emit('tauri://resize', {}); }
          return null;
        }
        const key = args.accountKey as string;
        const other = key === 'account-b';
        const localTokens = other ? { ...tokens, inputTokens: 4500, totalTokens: 5000 } : tokens;
        if (command === 'get_accounts') return { activeAccount: profiles.find(p => p.accountKey === active) ?? null, accounts: profiles };
        if (command === 'get_switch_accounts') {
          const flags = globalThis as unknown as { __meterDisconnected?: boolean; __meterBusy?: boolean; __meterDiskMismatch?: boolean };
          return { profiles: savedLogins, activeAccount: profiles.find(p => p.accountKey === (flags.__meterDiskMismatch ? 'account-b' : active)) ?? null, loginPending, live: { enabled: liveEnabled, connected: flags.__meterDisconnected ? 0 : 1, busy: !!flags.__meterBusy, runtimeAccount: profiles.find(p => p.accountKey === active) ?? null } };
        }
        if (command === 'configure_live_switching') { liveEnabled = args.enabled as boolean; return null; }
        if (command === 'save_current_login') { const saved = savedLogins.find(p => p.account.accountKey === active)!; saved.label = args.label as string || saved.label; return saved; }
        if (command === 'start_account_login') { loginPending = true; return null; }
        if (command === 'poll_account_login') return null;
        if (command === 'cancel_account_login') { loginPending = false; return null; }
        if (command === 'remove_saved_login') { savedLogins = savedLogins.filter(p => p.id !== args.id); return null; }
        if (command === 'switch_codex_account') {
          if ((globalThis as unknown as { __meterSwitchFail?: boolean }).__meterSwitchFail) throw new Error('The running Codex engine still uses another login. The previous desktop login was restored.');
          active = savedLogins.find(p => p.id === args.id)!.account.accountKey;
          emit('accounts-updated', { activeAccount: profiles.find(p => p.accountKey === active), accounts: profiles });
          return { account: profiles.find(p => p.accountKey === active), message: `Codex is now using ${savedLogins.find(p => p.id === args.id)!.label}. Confirmed by its running engine; Codex stayed open.` };
        }
        if (command === 'get_guard_integration') return { installed: guardInstalled };
        if (command === 'configure_usage_guard') { guardInstalled = args.enabled as boolean; return { installed: guardInstalled }; }
        if (command === 'get_usage_guard') {
          const low = (globalThis as unknown as { __meterLowQuota?: boolean }).__meterLowQuota;
          return low ? { status: 'pause', shouldPause: true, remainingPercent: 5, resumeAt: '2026-10-06T11:02:00+00:00', prompt: 'Preserve a handoff and schedule this same chat at 2026-10-06T11:02:00+00:00.' } : { status: 'ready', shouldPause: false, remainingPercent: 80, prompt: null };
        }
        if (command === 'get_monitor_state') return { state: active ? 'monitoring' : 'authRequired', errorMessage: active ? null : 'Sign into Codex', detectedClients: [] };
        if (command === 'get_usage') {
          if (key === (globalThis as unknown as { __meterDelayAccount: string | null }).__meterDelayAccount) await new Promise(resolve => globalThis.setTimeout(resolve, 1200));
          if (key === '__signed_out__' || key === 'default') return null;
          const scopedWindow = other ? { ...window, usedPercent: 70, remainingPercent: 30 } : window;
          const weekly = { ...scopedWindow, source: 'secondary', name: 'weekly', durationMinutes: 10080, usedPercent: other ? 75 : 40, remainingPercent: other ? 25 : 60, resetsAt: new Date((reset + 432000) * 1000).toISOString() };
          return { accountKey: key, capturedAt: new Date().toISOString(), limitId: 'codex', limitName: null, planType: other ? 'pro' : 'plus', windows: [scopedWindow, weekly], limits: [{ limitId: 'codex', limitName: null, windows: [scopedWindow, weekly] }], credits: null };
        }
        if (command === 'get_token_totals') return { accountKey: key, currentSession: localTokens, fiveHourWindow: localTokens, weeklyWindow: localTokens, today: localTokens, currentMonth: localTokens, allTimeRecorded: localTokens };
        if (command === 'get_account_usage') return key === '__signed_out__' ? null : { accountKey: key, summary: { lifetimeTokens: other ? 90000 : (globalThis as unknown as { __meterAccountTokens?: number }).__meterAccountTokens ?? 9000 }, dailyUsageBuckets: [{ startDate: '2026-10-02', tokens: other ? 80000 : 8000 }], fetchedAt: Math.floor(Date.now() / 1000) };
        if (command === 'get_account_usage_days') return key === '__signed_out__' ? { days: [], total: 0 } : { days: [{ startDate: '2026-10-02', tokens: other ? 80000 : 8000, observedAt: Math.floor(Date.now() / 1000) }], total: 1 };
        if (command === 'get_current_chat_summary') return key === '__signed_out__' ? null : { accountKey: key, id: 'fixture-chat', title: 'Example chat', model: 'future-model', reasoningEffort: 'high', usage: { ...localTokens, cacheWriteInputTokens: 0 }, cacheRate: 33.3, estimatedCostUsd: 0.02, updatedAt: reset - 3600, turnCount: 1, requestCount: 1, isCurrent: true };
        if (command === 'get_quota_history') {
          const mode = (globalThis as unknown as { __meterHistoryMode?: string }).__meterHistoryMode;
          if (key === '__signed_out__' || mode === 'empty') return [];
          const points = [30, 35, 40].map((value, index) => ({ capturedAt: reset - 10800 + index * 3600, windowKind: 'weekly', usedPercent: other ? value + 35 : value, remainingPercent: other ? 65 - value : 100 - value }));
          return mode === 'single' ? points.slice(-1) : points;
        }
        if (command === 'get_recent_events') return [{ eventType: 'connected', label: 'Connected to Codex', timestamp: '19:39:56' }, { eventType: 'monitor_started', label: 'Monitoring started', timestamp: '19:39:55' }];
        if (command === 'get_chat_sessions') return [];
        if (command === 'get_limit_history') { const base = key === '__signed_out__' ? [] : other ? periods.slice(0, 1).map(p => ({ ...p, usedPercent: 70, tokens: localTokens })) : periods; const rows = base.filter(p => (!args.limitId || p.limitId === args.limitId) && (!args.windowKind || p.windowKind === args.windowKind)); return { periods: rows, total: rows.length }; }
        if (command === 'get_pricing') return catalog;
        if (command === 'refresh_pricing') { catalog.models['future-model'].standard.input = 3; emit('pricing-updated', catalog); return catalog; }
        if (command === 'get_usage_deltas') return { sessionDelta: 0, todayDelta: 0, peakHourUsed: 40, sessionsToday: 1, longestSessionMinutes: 10 };
        return null;
      },
    } });
  });
});

test('Switch changes the live account and preserves separate quota history', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('navigation').getByRole('button', { name: 'Switch', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Switch accounts' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Switch to Personal' })).toBeDisabled();
  await page.getByRole('button', { name: 'Switch to Work' }).click();
  await expect(page.getByRole('status')).toContainText('Codex is now using Work');
  await expect(page.getByRole('button', { name: 'Switch to Work' })).toBeDisabled();
  await page.getByRole('navigation').getByRole('button', { name: 'Overview', exact: true }).click();
  await expect(page.getByRole('progressbar', { name: '5-hour quota remaining' })).toHaveAttribute('aria-valuenow', '30');
});

test('Failed engine confirmation shows an error and keeps the original account active', async ({ page }) => {
  await page.addInitScript(() => Object.assign(globalThis, { __meterSwitchFail: true }));
  await page.goto('/');
  await page.getByRole('navigation').getByRole('button', { name: 'Switch', exact: true }).click();
  await page.getByRole('button', { name: 'Switch to Work' }).click();
  await expect(page.getByRole('alert')).toContainText('previous desktop login was restored');
  await expect(page.getByRole('button', { name: 'Switch to Personal' })).toBeDisabled();
});

test('Disk login mismatch does not mark the target as the current desktop account', async ({ page }) => {
  await page.addInitScript(() => Object.assign(globalThis, { __meterDiskMismatch: true }));
  await page.goto('/');
  await page.getByRole('navigation').getByRole('button', { name: 'Switch', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Switch to Work' })).toBeEnabled();
  await page.getByRole('button', { name: 'Switch to Work' }).click();
  await expect(page.getByRole('status')).toContainText('Codex is now using Work');
});

test('Disconnected desktop explains the one-time setup and prevents a false switch', async ({ page }) => {
  await page.addInitScript(() => Object.assign(globalThis, { __meterDisconnected: true }));
  await page.goto('/');
  await page.getByRole('navigation').getByRole('button', { name: 'Switch', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Switch to Work' })).toBeDisabled();
  await expect(page.getByText(/Waiting for desktop connection/)).toBeVisible();
  await page.getByRole('button', { name: 'Disable live switching' }).click();
  await expect(page.getByRole('status')).toContainText('Existing chats remain usable');
  await page.getByRole('button', { name: 'Enable live switching' }).click();
  await expect(page.getByRole('status')).toContainText('Quit and reopen Codex normally once');
});

test('Active work or voice blocks switching while keeping Codex connected', async ({ page }) => {
  await page.addInitScript(() => Object.assign(globalThis, { __meterBusy: true }));
  await page.goto('/');
  await page.getByRole('navigation').getByRole('button', { name: 'Switch', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Switch to Work' })).toBeDisabled();
  await expect(page.getByText('Desktop connected. Finish or stop active work or voice before switching.')).toBeVisible();
});

test('New login can be cancelled without changing the active account', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('navigation').getByRole('button', { name: 'Switch', exact: true }).click();
  await page.getByLabel('Account name').fill('Second account');
  await page.getByRole('button', { name: 'Add another account' }).click();
  await expect(page.getByText('Waiting for browser sign-in…')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Switch to Work' })).toBeDisabled();
  await page.getByRole('button', { name: 'Cancel sign-in' }).click();
  await expect(page.getByRole('button', { name: 'Switch to Work' })).toBeEnabled();
});

test('Usage Guard shows exact resume time and installs or removes the connection', async ({ page }) => {
  await page.addInitScript(() => Object.assign(globalThis, { __meterLowQuota: true }));
  await page.setViewportSize({ width: 900, height: 650 });
  await page.goto('/');
  await page.getByRole('navigation').getByRole('button', { name: 'Usage Guard', exact: true }).click();
  await expect(page.getByText('5.0%', { exact: true })).toBeVisible();
  await expect(page.getByText('Exact time: 2026-10-06T11:02:00+00:00')).toBeVisible();
  await page.getByRole('button', { name: 'Enable Usage Guard' }).click();
  await expect(page.getByRole('status')).toContainText('review/trust');
  await page.getByRole('button', { name: 'Remove connection' }).click();
  await expect(page.getByRole('button', { name: 'Enable Usage Guard' })).toBeVisible();
  expect(await page.locator('main').evaluate(el => el.scrollWidth > el.clientWidth)).toBe(false);
  await page.screenshot({ path: 'test-results/usage-guard.png', fullPage: true });
});

test('One title bar sends minimize, maximize, restore and close-to-tray actions', async ({ page }) => {
  await page.goto('/');
  const controls = page.getByRole('toolbar', { name: 'Window controls' });
  await expect(controls).toHaveCount(1);
  await expect(controls.getByRole('button')).toHaveCount(3);
  await expect(page.getByText('Codex Meter', { exact: true })).toHaveCount(1);
  await controls.getByRole('button', { name: 'Maximize window' }).click();
  await expect(controls.getByRole('button', { name: 'Restore window' })).toBeVisible();
  await controls.getByRole('button', { name: 'Restore window' }).click();
  await expect(controls.getByRole('button', { name: 'Maximize window' })).toBeVisible();
  await controls.getByRole('button', { name: 'Minimize window' }).click();
  await controls.getByRole('button', { name: 'Close window' }).click();
  const commands = await page.evaluate(() => (globalThis as unknown as { __meterWindowCommands: string[] }).__meterWindowCommands);
  expect(commands).toEqual(['plugin:window|toggle_maximize', 'plugin:window|toggle_maximize', 'plugin:window|minimize', 'plugin:window|hide']);
});

for (const viewport of [{ width: 900, height: 650 }, { width: 1920, height: 1080 }]) {
  test(`Overview shows both quotas and a visible weekly plot at ${viewport.width}px`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await page.goto('/');
    await expect(page.getByRole('progressbar', { name: '5-hour quota remaining' })).toHaveAttribute('aria-valuenow', '80');
    await expect(page.getByRole('progressbar', { name: 'Weekly quota remaining' })).toHaveAttribute('aria-valuenow', '60');
    await expect(page.getByRole('region', { name: 'Tokens used' }).getByText('Lifetime · server', { exact: true })).toBeVisible();
    const chart = page.getByRole('region', { name: 'Weekly quota observations' });
    await expect(chart.locator('.recharts-line-curve')).toBeVisible();
    await expect(chart.getByText('100%', { exact: true })).toBeVisible();
    const plot = await chart.locator('.recharts-surface').boundingBox();
    expect(plot?.height).toBeGreaterThan(150);
    const horizontalOverflow = await page.locator('main').evaluate(el => el.scrollWidth > el.clientWidth);
    expect(horizontalOverflow).toBe(false);
    await page.screenshot({ path: `test-results/overview-${viewport.width}.png`, fullPage: true });
    await chart.scrollIntoViewIfNeeded();
    await page.screenshot({ path: `test-results/weekly-chart-${viewport.width}.png`, fullPage: true });
    await page.getByRole('button', { name: 'Open settings', exact: true }).click();
    await expect(page.getByRole('heading', { level: 1, name: 'Settings', exact: true })).toBeVisible();
  });
}

test('A single weekly observation renders a dot; an empty history explains the gap', async ({ page }) => {
  await page.addInitScript(() => Object.assign(globalThis, { __meterHistoryMode: 'single' }));
  await page.goto('/');
  const chart = page.getByRole('region', { name: 'Weekly quota observations' });
  await expect(chart.locator('.recharts-line-dot')).toBeVisible();
  await page.screenshot({ path: 'test-results/weekly-single-observation.png', fullPage: true });
  await page.addInitScript(() => Object.assign(globalThis, { __meterHistoryMode: 'empty' }));
  await page.reload();
  await expect(chart.getByText('No quota observations yet', { exact: true })).toBeVisible();
  await expect(chart.getByText('History builds while Meter is running.', { exact: true })).toBeVisible();
});

test('Limits retains completed windows, token totals, and bucket filters', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Limits', exact: true }).click();
  const table = page.getByRole('table', { name: 'Limit period token history' });
  await expect(table.getByRole('row')).toHaveCount(5);
  await expect(table.getByRole('columnheader')).toHaveText(['Limit / window', 'Start', 'Reset', 'Total tokens', 'Observed quota consumed']);
  await expect(table.getByRole('row').filter({ hasText: 'codex' }).filter({ hasText: 'Weekly' })).toHaveCount(1);
  await expect(table.getByText('completed', { exact: true })).toBeVisible();
  await expect(table.getByText('250', { exact: true })).toBeVisible();
  await page.screenshot({ path: 'test-results/limits.png', fullPage: true });
  await page.getByLabel('Window duration').selectOption('weekly');
  await expect(table.getByRole('row')).toHaveCount(3);
  await page.getByLabel('Limit bucket').fill('new-model-limit');
  await expect(table.getByRole('row')).toHaveCount(2);
  await expect(table.getByText('250', { exact: true })).toBeVisible();
});

test('Pricing displays new models and responds to a price update', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('navigation').getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByText('1 models · View rates per million tokens').click();
  await expect(page.getByRole('cell', { name: 'future-model', exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: '$2', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Update prices', exact: true }).click();
  await expect(page.getByRole('cell', { name: '$3', exact: true })).toBeVisible();
  await page.screenshot({ path: 'test-results/pricing.png', fullPage: true });
});

test('Limits separates server-wide activity from exact local window tokens', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Limits', exact: true }).click();
  const coverage = page.getByRole('region', { name: 'Account-wide usage coverage' });
  await expect(coverage.getByText('9,000', { exact: true })).toBeVisible();
  await expect(coverage.getByText(/even with no local Codex window open/)).toBeVisible();
  await coverage.getByText('1 saved days · Server-reported tokens').click();
  await expect(page.getByRole('table', { name: 'Server-reported daily tokens' }).getByText('8,000', { exact: true })).toBeVisible();
  const local = page.getByRole('table', { name: 'Limit period token history' });
  await expect(local.getByRole('columnheader', { name: 'Total tokens', exact: true })).toBeVisible();
  await expect(local.getByText('9,000', { exact: true })).toHaveCount(0);
  await page.screenshot({ path: 'test-results/account-coverage.png', fullPage: true });
});

for (const reported of [0, 50]) test(`An incomplete server summary (${reported}) cannot replace known local lifetime usage`, async ({ page }) => {
  await page.addInitScript(value => Object.assign(globalThis, { __meterAccountTokens: value }), reported);
  await page.goto('/');
  const summary = page.getByRole('region', { name: 'Tokens used' });
  await expect(summary.getByText('Lifetime · local', { exact: true })).toBeVisible();
  await expect(summary.getByText('Local totals · account total unavailable or incomplete', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Insights', exact: true }).click();
  await expect(page.getByText('Lifetime Total', { exact: true }).locator('..').getByText('100', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Limits', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Account-wide usage coverage' }).getByText(/may be delayed or incomplete/)).toBeVisible();
});

test('Account selector shows separate saved quotas, period tokens and day totals', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Limits', exact: true }).click();
  const coverage = page.getByRole('region', { name: 'Account-wide usage coverage' });
  await expect(coverage.getByText('9,000', { exact: true })).toBeVisible();
  await page.getByLabel('Account', { exact: true }).selectOption('account-b');
  await expect(page.getByText('Saved history', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Refresh usage data' })).toBeDisabled();
  await expect(coverage.getByText('90,000', { exact: true })).toBeVisible();
  const local = page.getByRole('table', { name: 'Limit period token history' });
  await expect(local.getByRole('row')).toHaveCount(2);
  await expect(local.getByText('5,000', { exact: true })).toBeVisible();
  await expect(local.getByText('70.0%', { exact: true })).toBeVisible();
  await coverage.getByText('1 saved days · Server-reported tokens').click();
  await expect(page.getByRole('table', { name: 'Server-reported daily tokens' }).getByText('80,000', { exact: true })).toBeVisible();
  await page.screenshot({ path: 'test-results/separate-accounts.png', fullPage: true });
  await page.getByLabel('Account', { exact: true }).selectOption('');
  await expect(coverage.getByText('9,000', { exact: true })).toBeVisible();
  await expect(local.getByRole('row')).toHaveCount(5);
});

test('Codex sign-in changes follow the account and ignore old notifications; sign-out clears data', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Limits', exact: true }).click();
  const coverage = page.getByRole('region', { name: 'Account-wide usage coverage' });
  await expect(coverage.getByText('9,000', { exact: true })).toBeVisible();
  await page.evaluate(() => (globalThis as unknown as { __meterSignIn: (key: string | null) => void }).__meterSignIn('account-b'));
  await expect(coverage.getByText('90,000', { exact: true })).toBeVisible();
  await page.evaluate(() => (globalThis as unknown as { __meterEmit: (name: string, payload: unknown) => void }).__meterEmit('account-usage-updated', { accountKey: 'account-a', summary: { lifetimeTokens: 123456789 } }));
  await expect(coverage.getByText('90,000', { exact: true })).toBeVisible();
  await expect(coverage.getByText('123,456,789', { exact: true })).toHaveCount(0);
  await page.evaluate(() => (globalThis as unknown as { __meterSignIn: (key: string | null) => void }).__meterSignIn(null));
  await expect(page.getByLabel('Account', { exact: true }).getByRole('option', { name: 'No account signed in' })).toHaveCount(1);
  await expect(coverage.getByText('90,000', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('table', { name: 'Limit period token history' }).getByRole('row')).toHaveCount(1);
});

test('A delayed saved-account response cannot overwrite the newly selected account', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Limits', exact: true }).click();
  const coverage = page.getByRole('region', { name: 'Account-wide usage coverage' });
  await expect(coverage.getByText('9,000', { exact: true })).toBeVisible();
  await page.evaluate(() => Object.assign(globalThis, { __meterDelayAccount: 'account-b' }));
  await page.getByLabel('Account', { exact: true }).selectOption('account-b');
  await page.getByLabel('Account', { exact: true }).selectOption('');
  await expect(coverage.getByText('9,000', { exact: true })).toBeVisible();
  await page.waitForTimeout(1500);
  await expect(coverage.getByText('9,000', { exact: true })).toBeVisible();
  await expect(coverage.getByText('90,000', { exact: true })).toHaveCount(0);
});
