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
    Object.assign(globalThis, {
      __meterEmit: emit,
      __meterSignIn: (key: string | null) => { active = key; emit('accounts-updated', { activeAccount: profiles.find(p => p.accountKey === key) ?? null, accounts: profiles }); emit('state-changed', key ? 'monitoring' : 'authRequired'); },
      __meterDelayAccount: null,
    });
    Object.assign(globalThis, { __TAURI_INTERNALS__: {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
      transformCallback: (callback: (event: unknown) => void) => { callbacks.set(++id, callback); return id; },
      unregisterCallback: (callback: number) => callbacks.delete(callback),
      invoke: async (command: string, args: Record<string, unknown> = {}) => {
        if (command === 'plugin:event|listen') { const list = listeners.get(args.event as string) ?? []; list.push(args.handler as number); listeners.set(args.event as string, list); return ++id; }
        const key = args.accountKey as string;
        const other = key === 'account-b';
        const localTokens = other ? { ...tokens, inputTokens: 4500, totalTokens: 5000 } : tokens;
        if (command === 'get_accounts') return { activeAccount: profiles.find(p => p.accountKey === active) ?? null, accounts: profiles };
        if (command === 'get_monitor_state') return { state: active ? 'monitoring' : 'authRequired', errorMessage: active ? null : 'Sign into Codex', detectedClients: [] };
        if (command === 'get_usage') {
          if (key === (globalThis as unknown as { __meterDelayAccount: string | null }).__meterDelayAccount) await new Promise(resolve => globalThis.setTimeout(resolve, 1200));
          if (key === '__signed_out__' || key === 'default') return null;
          const scopedWindow = other ? { ...window, usedPercent: 70, remainingPercent: 30 } : window;
          return { accountKey: key, capturedAt: new Date().toISOString(), limitId: 'codex', limitName: null, planType: other ? 'pro' : 'plus', windows: [scopedWindow], limits: [{ limitId: 'codex', limitName: null, windows: [scopedWindow] }], credits: null };
        }
        if (command === 'get_token_totals') return { accountKey: key, currentSession: localTokens, fiveHourWindow: localTokens, weeklyWindow: localTokens, today: localTokens, currentMonth: localTokens, allTimeRecorded: localTokens };
        if (command === 'get_account_usage') return key === '__signed_out__' ? null : { accountKey: key, summary: { lifetimeTokens: other ? 90000 : (globalThis as unknown as { __meterAccountTokens?: number }).__meterAccountTokens ?? 9000 }, dailyUsageBuckets: [{ startDate: '2026-10-02', tokens: other ? 80000 : 8000 }], fetchedAt: Math.floor(Date.now() / 1000) };
        if (command === 'get_account_usage_days') return key === '__signed_out__' ? { days: [], total: 0 } : { days: [{ startDate: '2026-10-02', tokens: other ? 80000 : 8000, observedAt: Math.floor(Date.now() / 1000) }], total: 1 };
        if (command === 'get_quota_history' || command === 'get_recent_events' || command === 'get_chat_sessions') return [];
        if (command === 'get_limit_history') { const base = key === '__signed_out__' ? [] : other ? periods.slice(0, 1).map(p => ({ ...p, usedPercent: 70, tokens: localTokens })) : periods; const rows = base.filter(p => (!args.limitId || p.limitId === args.limitId) && (!args.windowKind || p.windowKind === args.windowKind)); return { periods: rows, total: rows.length }; }
        if (command === 'get_pricing') return catalog;
        if (command === 'refresh_pricing') { catalog.models['future-model'].standard.input = 3; emit('pricing-updated', catalog); return catalog; }
        if (command === 'get_usage_deltas') return { sessionDelta: 0, todayDelta: 0, peakHourUsed: 40, sessionsToday: 1, longestSessionMinutes: 10 };
        return null;
      },
    } });
  });
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
