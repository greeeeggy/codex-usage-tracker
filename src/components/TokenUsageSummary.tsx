import { useUsageStore } from '../stores/usageStore';
import { AccountUsage, ChatSessionSummary, TokenTotals } from '../types/usage';
import { formatNumber, formatUsd } from '../utils/cn';
import { lifetimeCoverage } from '../utils/usageCoverage';

interface TokenUsageSummaryProps {
  accountUsage: AccountUsage | null;
  tokenTotals: TokenTotals | null;
  currentChat: ChatSessionSummary | null;
}

export function TokenUsageSummary({ accountUsage, tokenTotals, currentChat }: TokenUsageSummaryProps) {
  const pricing = useUsageStore(s => s.pricing);
  const snapshot = useUsageStore(s => s.snapshot);
  const { usableReport, value: lifetimeTokens } = lifetimeCoverage(accountUsage, tokenTotals, snapshot);
  const metrics = [
    { label: 'Current chat', value: currentChat?.usage.totalTokens ?? tokenTotals?.currentSession.totalTokens ?? null },
    { label: '5-hour · local', value: tokenTotals?.fiveHourWindow.totalTokens ?? null },
    { label: 'Weekly · local', value: tokenTotals?.weeklyWindow.totalTokens ?? null },
    { label: 'Today · local', value: tokenTotals?.today.totalTokens ?? null },
    { label: usableReport ? 'Lifetime · server' : 'Lifetime · local', value: lifetimeTokens },
  ];
  return (
    <section className="token-panel" aria-label="Tokens used">
      <div className="section-heading">
        <h2>Tokens</h2>
        <span>{usableReport ? 'Lifetime reported by Codex' : 'Local totals · account total unavailable or incomplete'}</span>
      </div>
      <dl className="token-metrics">
        {metrics.map(metric => <div key={metric.label}>
          <dt>{metric.label}</dt>
          <dd title={metric.value !== null ? metric.value.toLocaleString() + ' tokens' : 'Unavailable'}>{metric.value !== null ? formatNumber(metric.value) : '—'}</dd>
        </div>)}
      </dl>
      {currentChat && <div className="chat-usage-row" aria-label="Current chat breakdown">
        <span className="chat-model">{currentChat.model ?? 'Unknown model'}{currentChat.reasoningEffort ? ' · ' + currentChat.reasoningEffort : ''}</span>
        <span>Input <b>{formatNumber(currentChat.usage.inputTokens)}</b></span>
        <span>Output <b>{formatNumber(currentChat.usage.outputTokens)}</b></span>
        <span>Cache <b>{currentChat.cacheRate.toFixed(1)}%</b></span>
        <span>API estimate <b>{formatUsd(currentChat.estimatedCostUsd)}</b></span>
      </div>}
      <details className="usage-notes">
        <summary>About these counts and estimates</summary>
        <p>Quota is shared across devices. Window token counts cover local requests with limit attribution; exact remote tokens per window are unavailable. Five-hour and weekly totals overlap. Server totals include local activity.</p>
        <p>{pricing?.fetchedAt ? 'API estimates use standard published prices, updated ' + new Date(pricing.fetchedAt * 1000).toLocaleString() + (pricing.lastError ? '. Using saved prices.' : '.') : 'API prices are unavailable until the first successful update.'} Estimates are separate from subscription charges.</p>
      </details>
    </section>
  );
}
