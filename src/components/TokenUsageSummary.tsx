import { Hash } from 'lucide-react';
import { AccountUsage, ChatSessionSummary, TokenTotals } from '../types/usage';
import { formatNumber, formatUsd } from '../utils/cn';

interface TokenUsageSummaryProps {
  accountUsage: AccountUsage | null;
  tokenTotals: TokenTotals | null;
  currentChat: ChatSessionSummary | null;
}

export function TokenUsageSummary({ accountUsage, tokenTotals, currentChat }: TokenUsageSummaryProps) {
  const authoritativeLifetime = accountUsage?.summary?.lifetimeTokens;
  const lifetimeTokens = authoritativeLifetime ?? tokenTotals?.allTimeRecorded.totalTokens ?? null;
  const metrics = [
    { label: 'Current chat', value: currentChat?.usage.totalTokens ?? tokenTotals?.currentSession.totalTokens ?? null },
    { label: '5-hour window', value: tokenTotals?.fiveHourWindow.totalTokens ?? null },
    { label: 'This week', value: tokenTotals?.weeklyWindow.totalTokens ?? null },
    { label: 'Today', value: tokenTotals?.today.totalTokens ?? null },
    { label: 'Lifetime', value: lifetimeTokens, accent: true },
  ];

  return (
    <section
      className="rounded-xl p-5"
      style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}
      aria-label="Tokens used"
    >
      <div className="flex items-center justify-between gap-4 mb-4">
        <div className="flex items-center gap-2">
          <Hash size={16} style={{ color: 'var(--purple)' }} />
          <h3 className="text-[15px] font-semibold" style={{ color: 'var(--text-primary)' }}>
            Tokens Used
          </h3>
        </div>
        <span className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
          {authoritativeLifetime !== null && authoritativeLifetime !== undefined
            ? 'Lifetime total from Codex'
            : 'Local totals'}
        </span>
      </div>

      {currentChat && (
        <div
          className="rounded-lg px-4 py-3 mb-3 grid grid-cols-2 md:grid-cols-5 gap-3"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}
        >
          <div className="col-span-2 md:col-span-1 min-w-0">
            <span className="text-[10px] uppercase tracking-wider block mb-1" style={{ color: 'var(--text-muted)' }}>
              Model
            </span>
            <span className="text-sm font-semibold truncate block" style={{ color: 'var(--text-primary)' }}>
              {currentChat.model ?? 'Unknown'}
              {currentChat.reasoningEffort ? ` · ${currentChat.reasoningEffort}` : ''}
            </span>
          </div>
          <div>
            <span className="text-[10px] uppercase tracking-wider block mb-1" style={{ color: 'var(--text-muted)' }}>
              Input
            </span>
            <span className="text-sm font-mono font-semibold" style={{ color: 'var(--text-primary)' }}>
              {formatNumber(currentChat.usage.inputTokens)}
            </span>
          </div>
          <div>
            <span className="text-[10px] uppercase tracking-wider block mb-1" style={{ color: 'var(--text-muted)' }}>
              Output
            </span>
            <span className="text-sm font-mono font-semibold" style={{ color: 'var(--text-primary)' }}>
              {formatNumber(currentChat.usage.outputTokens)}
            </span>
          </div>
          <div>
            <span className="text-[10px] uppercase tracking-wider block mb-1" style={{ color: 'var(--text-muted)' }}>
              Cache rate
            </span>
            <span className="text-sm font-mono font-semibold" style={{ color: 'var(--green)' }}>
              {currentChat.cacheRate.toFixed(1)}%
            </span>
          </div>
          <div>
            <span className="text-[10px] uppercase tracking-wider block mb-1" style={{ color: 'var(--text-muted)' }}>
              API est.
            </span>
            <span className="text-sm font-mono font-semibold" style={{ color: 'var(--purple-bright)' }}>
              {formatUsd(currentChat.estimatedCostUsd)}
            </span>
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 md:grid-cols-5 gap-3">
        {metrics.map((metric) => (
          <div
            key={metric.label}
            className="rounded-lg px-4 py-3 min-w-0"
            style={{
              background: metric.accent ? 'var(--purple-dim)' : 'var(--bg-elevated)',
              border: metric.accent
                ? '1px solid rgba(139, 92, 246, 0.28)'
                : '1px solid var(--border-subtle)',
            }}
          >
            <span className="text-[11px] block mb-1 truncate" style={{ color: 'var(--text-muted)' }}>
              {metric.label}
            </span>
            <span
              className="text-xl font-semibold tabular-nums block truncate"
              style={{ color: metric.accent ? 'var(--purple-bright)' : 'var(--text-primary)' }}
              title={metric.value !== null ? `${metric.value.toLocaleString()} tokens` : 'No data'}
            >
              {metric.value !== null ? formatNumber(metric.value) : '—'}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}
