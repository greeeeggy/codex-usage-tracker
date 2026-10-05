import { useEffect } from 'react';
import { useUsageStore } from '../stores/usageStore';
import { DashboardHeader } from '../components/DashboardHeader';
import { WeeklyLimitCard } from '../components/WeeklyLimitCard';
import { WeeklyUsageChart } from '../components/HistoryCharts';
import { MetricCardGrid } from '../components/MetricCardGrid';
import { RecentEventsCard } from '../components/RecentEventsCard';
import { LimitSummaryCard } from '../components/LimitSummaryCard';
import { TokenUsageSummary } from '../components/TokenUsageSummary';
import { LimitPeriodHistory } from '../components/LimitPeriodHistory';
import { AccountUsageCoverage } from '../components/AccountUsageCoverage';
import { PricingCatalogCard } from '../components/PricingCatalogCard';
import { ChatSessionHistory } from '../components/ChatSessionHistory';
import { Activity, RefreshCw, Layers, History as HistoryIcon, Gauge, MonitorDot, Lightbulb, Settings as SettingsIcon, Cpu } from 'lucide-react';
import { formatNumber, formatUsd } from '../utils/cn';
import { lifetimeCoverage } from '../utils/usageCoverage';

export function Dashboard() {
  const init = useUsageStore((s) => s.init);
  const refresh = useUsageStore((s) => s.refresh);
  const snapshot = useUsageStore((s) => s.snapshot);
  const accountUsage = useUsageStore((s) => s.accountUsage);
  const currentChat = useUsageStore((s) => s.currentChat);
  const tokenTotals = useUsageStore((s) => s.tokenTotals);
  const quotaHistory = useUsageStore((s) => s.quotaHistory);
  const usageDeltas = useUsageStore((s) => s.usageDeltas);
  const recentEvents = useUsageStore((s) => s.recentEvents);
  const monitorState = useUsageStore((s) => s.monitorState);
  const errorMessage = useUsageStore((s) => s.errorMessage);
  const detectedClients = useUsageStore((s) => s.detectedClients);
  const activePage = useUsageStore((s) => s.activePage);
  const setActivePage = useUsageStore((s) => s.setActivePage);
  const accountKey = useUsageStore(s => s.viewAccountKey);
  const savedAccount = useUsageStore(s => s.selectedAccountKey !== null && s.selectedAccountKey !== s.activeAccount?.accountKey);

  useEffect(() => {
    init();

    const refreshWhenShown = () => {
      void init();
    };
    window.addEventListener('focus', refreshWhenShown);
    document.addEventListener('visibilitychange', refreshWhenShown);

    return () => {
      window.removeEventListener('focus', refreshWhenShown);
      document.removeEventListener('visibilitychange', refreshWhenShown);
    };
  }, [init]);

  const weeklyWindow = snapshot?.windows.find(
    (w) => w.name === 'weekly' || w.durationMinutes === 10080
  );
  const fiveHourWindow = snapshot?.windows.find(
    (w) => w.name === 'fiveHour' || w.durationMinutes === 300
  );
  const currentChatUsage = currentChat?.usage ?? tokenTotals?.currentSession;
  const lifetime = lifetimeCoverage(accountUsage, tokenTotals, snapshot);

  // Error / auth required state
  if (!savedAccount && (monitorState === 'error' || monitorState === 'authRequired') && !['limits', 'history', 'settings'].includes(activePage)) {
    return (
      <div className="h-full p-6">
        <DashboardHeader />
        <div
          className="rounded-2xl p-10 text-center flex flex-col items-center justify-center"
          style={{
            background: 'var(--danger-dim)',
            border: '1px solid rgba(255, 77, 94, 0.2)',
            minHeight: 300,
          }}
        >
          <div
            className="w-14 h-14 rounded-full flex items-center justify-center mb-4"
            style={{ background: 'rgba(255, 77, 94, 0.15)' }}
          >
            <Activity size={24} style={{ color: 'var(--danger)' }} />
          </div>
          <h2 className="text-lg font-semibold mb-2" style={{ color: 'var(--text-primary)' }}>
            Connection Issue
          </h2>
          <p className="text-sm mb-6 max-w-md" style={{ color: 'var(--text-secondary)' }}>
            {errorMessage || 'Unable to connect to Codex. Please check your configuration.'}
          </p>
          <button
            onClick={refresh}
            className="flex items-center gap-2 px-5 py-2 rounded-lg text-sm font-medium transition-colors duration-150 cursor-pointer"
            style={{
              background: 'rgba(255,255,255,0.08)',
              color: 'var(--text-primary)',
              border: '1px solid var(--border-default)',
            }}
          >
            <RefreshCw size={14} />
            Retry Connection
          </button>
        </div>
      </div>
    );
  }

  // View rendering based on active sidebar tab
  const renderActiveView = () => {
    switch (activePage) {
      case 'usage':
        return (
          <div className="space-y-6">
            <h2 className="text-xl font-bold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
              <Layers size={20} style={{ color: 'var(--purple)' }} /> Detailed Usage & Token Breakdown
            </h2>

            {tokenTotals ? (
              <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                {/* Token Totals */}
                <div className="rounded-2xl p-6" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
                  <h3 className="text-sm font-semibold uppercase tracking-wider mb-4" style={{ color: 'var(--text-muted)' }}>
                    Token Usage Totals
                  </h3>
                  <div className="space-y-3">
                    {[
                      { label: 'Current Chat', value: currentChatUsage?.totalTokens ?? 0 },
                      { label: 'Current 5h Window', value: tokenTotals.fiveHourWindow.totalTokens },
                      { label: 'Current Week', value: tokenTotals.weeklyWindow.totalTokens },
                      { label: 'Today', value: tokenTotals.today.totalTokens },
                      { label: 'This Month', value: tokenTotals.currentMonth.totalTokens },
                      {
                        label: 'Lifetime (Codex Account)',
                        value: accountUsage?.summary?.lifetimeTokens
                          ?? tokenTotals.allTimeRecorded.totalTokens,
                      },
                      { label: 'Recorded Locally', value: tokenTotals.allTimeRecorded.totalTokens },
                    ].map((item, idx) => (
                      <div key={idx} className="flex justify-between items-center py-2 border-b border-white/5 last:border-0">
                        <span style={{ color: 'var(--text-secondary)' }}>{item.label}</span>
                        <span className="font-mono font-semibold tabular-nums" style={{ color: 'var(--text-primary)' }}>
                          {formatNumber(item.value)}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>

                {/* Current Chat Token Breakdown */}
                <div className="rounded-2xl p-6" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
                  <h3 className="text-sm font-semibold uppercase tracking-wider mb-4" style={{ color: 'var(--text-muted)' }}>
                    Current Chat Token Breakdown
                  </h3>
                  <div className="space-y-3">
                    {[
                      { label: 'Input Tokens', value: currentChatUsage?.inputTokens ?? 0 },
                      { label: 'Cached Input Tokens', value: currentChatUsage?.cachedInputTokens ?? 0 },
                      { label: 'Cache Write Tokens', value: currentChat?.usage.cacheWriteInputTokens ?? 0 },
                      { label: 'Uncached Input Tokens', value: currentChatUsage?.uncachedInputTokens ?? 0 },
                      { label: 'Output Tokens', value: currentChatUsage?.outputTokens ?? 0 },
                    ].map((item, idx) => (
                      <div key={idx} className="flex justify-between items-center py-2 border-b border-white/5 last:border-0">
                        <span style={{ color: 'var(--text-secondary)' }}>{item.label}</span>
                        <span className="font-mono font-semibold tabular-nums" style={{ color: 'var(--text-primary)' }}>
                          {formatNumber(item.value)}
                        </span>
                      </div>
                    ))}
                    {currentChatUsage?.reasoningTokens ? (
                      <div className="flex justify-between items-center py-2 border-b border-white/5 last:border-0">
                        <span style={{ color: 'var(--text-secondary)' }}>Reasoning Tokens</span>
                        <span className="font-mono font-semibold tabular-nums" style={{ color: 'var(--purple-bright)' }}>
                          {formatNumber(currentChatUsage.reasoningTokens)}
                        </span>
                      </div>
                    ) : null}
                    <div className="flex justify-between items-center py-2 border-b border-white/5">
                      <span style={{ color: 'var(--text-secondary)' }}>Input Cache Rate</span>
                      <span className="font-mono font-semibold tabular-nums" style={{ color: 'var(--green)' }}>
                        {currentChat ? `${currentChat.cacheRate.toFixed(1)}%` : '—'}
                      </span>
                    </div>
                    <div className="flex justify-between items-center py-2">
                      <span style={{ color: 'var(--text-secondary)' }}>API-equivalent Estimate</span>
                      <span className="font-mono font-semibold tabular-nums" style={{ color: 'var(--purple-bright)' }}>
                        {formatUsd(currentChat?.estimatedCostUsd)}
                      </span>
                    </div>
                    {currentChat && (
                      <p className="pt-2 text-[10px]" style={{ color: 'var(--text-muted)' }}>
                        {currentChat.model ?? 'Unknown model'}
                        {currentChat.reasoningEffort ? ` · ${currentChat.reasoningEffort}` : ''}
                      </p>
                    )}
                  </div>
                </div>

                {/* Latest Request & Context Load */}
                <div className="rounded-2xl p-6" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
                  <h3 className="text-sm font-semibold uppercase tracking-wider mb-4" style={{ color: 'var(--text-muted)' }}>
                    Latest Request Context Load
                  </h3>
                  {snapshot?.latestContextLoadPercent !== undefined && snapshot?.latestContextLoadPercent !== null ? (
                    <div className="space-y-4">
                      {/* Context load visual indicator */}
                      <div>
                        <div className="flex justify-between items-center mb-2">
                          <span style={{ color: 'var(--text-secondary)' }} className="text-xs font-semibold">Latest Request Context Load</span>
                          <span style={{ color: 'var(--purple-bright)' }} className="text-sm font-bold font-mono">
                            {snapshot.latestContextLoadPercent.toFixed(1)}%
                          </span>
                        </div>
                        <div className="w-full h-2 rounded-full" style={{ background: 'rgba(255, 255, 255, 0.08)', overflow: 'hidden' }}>
                          <div
                            className="h-full rounded-full transition-all duration-500 ease-out"
                            style={{
                              width: `${snapshot.latestContextLoadPercent}%`,
                              background: 'linear-gradient(90deg, var(--purple) 0%, var(--blue) 100%)',
                            }}
                          />
                        </div>
                      </div>

                      <div className="space-y-3 pt-2">
                        {[
                          { label: 'Model Context Window', value: snapshot.latestContextWindow ? formatNumber(snapshot.latestContextWindow) + ' tokens' : '—' },
                        ].map((item, idx) => (
                          <div key={idx} className="flex justify-between items-center py-2 border-b border-white/5">
                            <span style={{ color: 'var(--text-secondary)' }}>{item.label}</span>
                            <span className="font-semibold" style={{ color: 'var(--text-primary)' }}>
                              {item.value}
                            </span>
                          </div>
                        ))}

                        {snapshot.latestLastRequestTokens ? (
                          <>
                            <div className="pt-2 text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>
                              Last Request Token Breakdown
                            </div>
                            {[
                              { label: 'Input Tokens', value: formatNumber(snapshot.latestLastRequestTokens.inputTokens) },
                              { label: 'Cached Input Tokens', value: formatNumber(snapshot.latestLastRequestTokens.cachedInputTokens) },
                              { label: 'Uncached Input Tokens', value: formatNumber(snapshot.latestLastRequestTokens.uncachedInputTokens) },
                              { label: 'Output Tokens', value: formatNumber(snapshot.latestLastRequestTokens.outputTokens) },
                            ].map((item, idx) => (
                              <div key={idx} className="flex justify-between items-center py-2 border-b border-white/5 last:border-0">
                                <span style={{ color: 'var(--text-secondary)' }} className="pl-2">{item.label}</span>
                                <span className="font-mono font-semibold tabular-nums" style={{ color: 'var(--text-primary)' }}>
                                  {item.value}
                                </span>
                              </div>
                            ))}
                            {snapshot.latestLastRequestTokens.reasoningTokens ? (
                              <div className="flex justify-between items-center py-2 border-b border-white/5 last:border-0">
                                <span style={{ color: 'var(--text-secondary)' }} className="pl-2">Reasoning Tokens</span>
                                <span className="font-mono font-semibold tabular-nums" style={{ color: 'var(--purple-bright)' }}>
                                  {formatNumber(snapshot.latestLastRequestTokens.reasoningTokens)}
                                </span>
                              </div>
                            ) : null}
                            {currentChat?.latestRequest && (
                              <>
                                <div className="flex justify-between items-center py-2 border-b border-white/5">
                                  <span style={{ color: 'var(--text-secondary)' }} className="pl-2">Cache Rate</span>
                                  <span className="font-mono font-semibold tabular-nums" style={{ color: 'var(--green)' }}>
                                    {currentChat.latestRequest.cacheRate.toFixed(1)}%
                                  </span>
                                </div>
                                <div className="flex justify-between items-center py-2">
                                  <span style={{ color: 'var(--text-secondary)' }} className="pl-2">API-equivalent Estimate</span>
                                  <span className="font-mono font-semibold tabular-nums" style={{ color: 'var(--purple-bright)' }}>
                                    {formatUsd(currentChat.latestRequest.estimatedCostUsd)}
                                  </span>
                                </div>
                              </>
                            )}
                          </>
                        ) : null}
                      </div>
                    </div>
                  ) : (
                    <div className="py-8 text-center text-sm" style={{ color: 'var(--text-muted)' }}>
                      No active task context recorded yet. Start using Codex to see context load.
                    </div>
                  )}
                </div>
              </div>
            ) : (
              <div className="rounded-2xl p-8 text-center" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
                <span style={{ color: 'var(--text-muted)' }}>No token usage recorded yet. Start using Codex to see token stats.</span>
              </div>
            )}
          </div>
        );

      case 'history':
        return (
          <div className="space-y-6">
            <h2 className="text-xl font-bold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
              <HistoryIcon size={20} style={{ color: 'var(--purple)' }} /> Chat & Usage History
            </h2>
            <ChatSessionHistory />
            <div>
              <h3 className="text-sm font-semibold uppercase tracking-wider mb-3" style={{ color: 'var(--text-muted)' }}>
                Weekly quota history
              </h3>
              <WeeklyUsageChart data={quotaHistory.map(h => ({ timestamp: new Date(h.capturedAt * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }), value: h.usedPercent }))} />
            </div>
          </div>
        );

      case 'limits':
        return (
          <div className="space-y-6">
            <h2 className="text-xl font-bold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
              <Gauge size={20} style={{ color: 'var(--purple)' }} /> Monitored Rate Limits
            </h2>
            {(snapshot?.limits ?? []).map(bucket => (
              <div key={bucket.limitId} className="space-y-3">
                <h3 className="text-sm font-semibold" style={{ color: 'var(--text-secondary)' }}>{bucket.limitName ?? bucket.limitId}</h3>
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">{bucket.windows.map(window => <WeeklyLimitCard key={window.source} window={window} />)}</div>
              </div>
            ))}
            <LimitPeriodHistory />
            <AccountUsageCoverage />
          </div>
        );

      case 'sessions':
        return (
          <div className="space-y-6">
            <h2 className="text-xl font-bold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
              <MonitorDot size={20} style={{ color: 'var(--purple)' }} /> Active Sessions
            </h2>

            <div className="rounded-2xl p-6" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
              <h3 className="text-sm font-semibold mb-4" style={{ color: 'var(--text-primary)' }}>
                Detected Codex Clients ({detectedClients.length})
              </h3>
              {detectedClients.length > 0 ? (
                <div className="space-y-3">
                  {detectedClients.map((client, idx) => (
                    <div key={idx} className="flex items-center justify-between p-3 rounded-lg" style={{ background: 'var(--bg-elevated)' }}>
                      <div className="flex items-center gap-3">
                        <Cpu size={18} style={{ color: 'var(--green)' }} />
                        <div>
                          <span className="font-medium text-sm block" style={{ color: 'var(--text-primary)' }}>{client.name}</span>
                          <span className="text-xs" style={{ color: 'var(--text-muted)' }}>{client.clientType}</span>
                        </div>
                      </div>
                      <span className="text-xs px-2 py-1 rounded bg-green-500/10 text-green-400 font-mono">ACTIVE</span>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="py-6 text-center" style={{ color: 'var(--text-muted)' }}>
                  No active Codex client processes detected.
                </div>
              )}
            </div>
          </div>
        );

      case 'insights':
        return (
          <div className="space-y-6">
            <h2 className="text-xl font-bold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
              <Lightbulb size={20} style={{ color: 'var(--purple)' }} /> Usage Insights
            </h2>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div className="rounded-2xl p-5" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
                <span className="text-xs font-medium block mb-1" style={{ color: 'var(--text-muted)' }}>Today's Total</span>
                <span className="text-2xl font-bold" style={{ color: 'var(--text-primary)' }}>
                  {tokenTotals?.today.totalTokens ? formatNumber(tokenTotals.today.totalTokens) : '0'}
                </span>
              </div>
              <div className="rounded-2xl p-5" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
                <span className="text-xs font-medium block mb-1" style={{ color: 'var(--text-muted)' }}>This Month Total</span>
                <span className="text-2xl font-bold" style={{ color: 'var(--text-primary)' }}>
                  {tokenTotals?.currentMonth.totalTokens ? formatNumber(tokenTotals.currentMonth.totalTokens) : '0'}
                </span>
              </div>
              <div className="rounded-2xl p-5" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
                <span className="text-xs font-medium block mb-1" style={{ color: 'var(--text-muted)' }}>Lifetime Total</span>
                <span className="text-2xl font-bold" style={{ color: 'var(--text-primary)' }}>
                  {lifetime.value !== null ? formatNumber(lifetime.value) : '—'}
                </span>
                <p className="text-xs mt-2" style={{ color: 'var(--text-muted)' }}>{lifetime.usableReport ? 'Reported by Codex' : 'Local records · account total unavailable or incomplete'}</p>
              </div>
            </div>
          </div>
        );

      case 'settings':
        return (
          <div className="space-y-6">
            <PricingCatalogCard />
            <h2 className="text-xl font-bold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
              <SettingsIcon size={20} style={{ color: 'var(--purple)' }} /> Application Settings
            </h2>

            <div className="rounded-2xl p-6 space-y-4" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
              <div className="flex items-center justify-between py-3 border-b border-white/5">
                <div>
                  <span className="font-semibold text-sm block" style={{ color: 'var(--text-primary)' }}>Monitoring Status</span>
                  <span className="text-xs" style={{ color: 'var(--text-muted)' }}>Current state of the Codex CLI rate limit monitor</span>
                </div>
                <span className="text-xs px-2.5 py-1 rounded-full font-mono uppercase font-bold" style={{ background: 'var(--purple-dim)', color: 'var(--purple-bright)' }}>
                  {monitorState}
                </span>
              </div>

              <div className="flex items-center justify-between py-3">
                <div>
                  <span className="font-semibold text-sm block" style={{ color: 'var(--text-primary)' }}>Refresh Monitoring Data</span>
                  <span className="text-xs" style={{ color: 'var(--text-muted)' }}>Force re-fetch rate limits from Codex</span>
                </div>
                <button
                  onClick={refresh}
                  className="px-4 py-1.5 rounded-lg text-xs font-medium transition-colors cursor-pointer"
                  style={{ background: 'var(--purple-dim)', color: 'var(--purple-bright)', border: '1px solid rgba(139, 92, 246, 0.3)' }}
                >
                  Refresh Now
                </button>
              </div>
            </div>
          </div>
        );

      case 'overview':
      default:
        return (
          <div className="space-y-4">
            {/* Both quota windows are primary information. */}
            <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
              <WeeklyLimitCard window={fiveHourWindow} compact />
              <WeeklyLimitCard window={weeklyWindow} compact />
            </div>

            <TokenUsageSummary accountUsage={accountUsage} tokenTotals={tokenTotals} currentChat={currentChat} />

            {/* Row: Chart + Metric cards */}
            <div className="grid grid-cols-1 xl:grid-cols-5 gap-4">
              <div className="xl:col-span-3">
                <WeeklyUsageChart
                  data={quotaHistory.map(h => ({
                    timestamp: new Date(h.capturedAt * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
                    value: h.usedPercent,
                  }))}
                  onViewHistory={() => setActivePage('history')}
                />
              </div>
              <div className="xl:col-span-2">
                <MetricCardGrid tokenTotals={tokenTotals} usageDeltas={usageDeltas} />
              </div>
            </div>

            {/* Row: Recent Events + Limit Summary */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
              <RecentEventsCard events={recentEvents as any} />
              <LimitSummaryCard
                fiveHourWindow={fiveHourWindow}
                weeklyWindow={weeklyWindow}
              />
            </div>
          </div>
        );
    }
  };

  return (
    <div className="p-6 space-y-4" style={{ animation: 'fade-in 300ms ease-out' }}>
      <DashboardHeader />
      {savedAccount && <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
        Showing saved usage for this account. Sign into it in Codex to update its quotas.
      </p>}
      <div key={accountKey}>{renderActiveView()}</div>
    </div>
  );
}
