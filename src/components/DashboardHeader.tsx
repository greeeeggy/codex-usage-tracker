import { useUsageStore } from '../stores/usageStore';
import { RefreshCw, Settings2 } from 'lucide-react';
import { format } from 'date-fns';
import { AccountSelector } from './AccountSelector';

export function DashboardHeader() {
  const snapshot = useUsageStore((s) => s.snapshot);
  const monitorState = useUsageStore((s) => s.monitorState);
  const errorMessage = useUsageStore((s) => s.errorMessage);
  const refresh = useUsageStore((s) => s.refresh);
  const isRefreshing = useUsageStore((s) => s.isRefreshing);
  const saved = useUsageStore(s => s.selectedAccountKey !== null && s.selectedAccountKey !== s.activeAccount?.accountKey);

  const planType = snapshot?.planType;

  let statusColor = 'var(--text-muted)';
  let statusLabel = 'Dormant';
  let showPulse = false;

  switch (monitorState) {
    case 'monitoring':
      statusColor = 'var(--green)';
      statusLabel = 'Monitoring';
      showPulse = true;
      break;
    case 'connecting':
      statusColor = 'var(--warning)';
      statusLabel = 'Connecting...';
      showPulse = true;
      break;
    case 'gracePeriod':
      statusColor = 'var(--warning)';
      statusLabel = 'Grace Period';
      break;
    case 'authRequired':
    case 'error':
      statusColor = 'var(--danger)';
      statusLabel = 'Error';
      break;
  }
  if (saved) { statusColor = 'var(--text-muted)'; statusLabel = 'Saved history'; showPulse = false; }

  return (
    <header className="flex flex-wrap gap-3 items-center justify-between mb-6">
      {/* Left side */}
      <div className="flex flex-wrap items-center gap-4">
        <h1 className="text-[26px] font-semibold tracking-tight" style={{ color: 'var(--text-primary)' }}>
          Codex Meter
        </h1>

        {planType && (
          <span
            className="text-[10px] font-bold tracking-widest uppercase px-2.5 py-1 rounded-md"
            style={{
              background: 'var(--purple-dim)',
              color: 'var(--purple-bright)',
              border: '1px solid rgba(139, 92, 246, 0.25)',
            }}
          >
            {planType}
          </span>
        )}

        <div className="flex items-center gap-2 ml-2" title={errorMessage || statusLabel}>
          <div className="relative">
            {showPulse && (
              <span
                className="absolute inset-0 rounded-full"
                style={{
                  background: statusColor,
                  animation: 'pulse-dot 2s ease-in-out infinite',
                  opacity: 0.4,
                }}
              />
            )}
            <span
              className="relative block w-2.5 h-2.5 rounded-full"
              style={{ background: statusColor }}
            />
          </div>
          <span className="text-sm font-medium" style={{ color: 'var(--text-secondary)' }}>
            {statusLabel}
          </span>
        </div>
      </div>

      {/* Right side */}
      <div className="flex items-center gap-3">
        <AccountSelector />
        {snapshot && (
          <div className="text-right mr-2">
            <span className="text-[11px] block" style={{ color: 'var(--text-muted)' }}>
              {saved ? 'Last observed' : 'Last refreshed'}
            </span>
            <span className="text-sm font-medium tabular-nums" style={{ color: 'var(--text-secondary)' }}>
              {format(new Date(snapshot.capturedAt), saved ? 'MMM d, h:mm a' : 'h:mm:ss a')}
            </span>
          </div>
        )}

        <button
          onClick={refresh}
          disabled={saved || isRefreshing || monitorState === 'error' || monitorState === 'authRequired'}
          className="p-2 rounded-lg transition-all duration-150 flex items-center justify-center"
          style={{
            color: isRefreshing ? 'var(--purple)' : 'var(--text-muted)',
            background: 'transparent',
            border: '1px solid var(--border-default)',
            opacity: monitorState === 'error' || monitorState === 'authRequired' ? 0.4 : 1,
            cursor: isRefreshing ? 'wait' : 'pointer',
          }}
          onMouseEnter={(e) => {
            if (!isRefreshing) {
              e.currentTarget.style.background = 'rgba(255,255,255,0.05)';
              e.currentTarget.style.color = 'var(--text-primary)';
            }
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'transparent';
            e.currentTarget.style.color = isRefreshing ? 'var(--purple)' : 'var(--text-muted)';
          }}
          aria-label="Refresh usage data"
          title="Refresh"
        >
          <RefreshCw
            size={16}
            style={isRefreshing ? { animation: 'spin 1s linear infinite' } : undefined}
          />
        </button>

        <button
          className="p-2 rounded-lg transition-all duration-150 flex items-center justify-center"
          style={{
            color: 'var(--text-muted)',
            background: 'transparent',
            border: '1px solid var(--border-default)',
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = 'rgba(255,255,255,0.05)';
            e.currentTarget.style.color = 'var(--text-primary)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'transparent';
            e.currentTarget.style.color = 'var(--text-muted)';
          }}
          aria-label="Settings"
          title="Settings"
        >
          <Settings2 size={16} />
        </button>
      </div>
    </header>
  );
}
