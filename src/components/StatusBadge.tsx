import { MonitorState } from '../types/usage';

interface StatusBadgeProps {
  state: MonitorState;
  errorMessage: string | null;
}

export function StatusBadge({ state, errorMessage }: StatusBadgeProps) {
  let color = 'var(--text-muted)';
  let label = 'Dormant';
  let pulse = false;

  switch (state) {
    case 'monitoring':
      color = 'var(--green)';
      label = 'Monitoring';
      pulse = true;
      break;
    case 'connecting':
      color = 'var(--warning)';
      label = 'Connecting...';
      pulse = true;
      break;
    case 'gracePeriod':
      color = 'var(--warning)';
      label = 'Grace Period';
      break;
    case 'authRequired':
    case 'error':
      color = 'var(--danger)';
      label = 'Error';
      break;
    case 'dormant':
      color = 'var(--text-muted)';
      label = 'Dormant';
      break;
  }

  return (
    <div className="flex items-center gap-2 group relative">
      <div className="relative flex h-2.5 w-2.5 items-center justify-center">
        {pulse && (
          <span
            className="absolute inline-flex h-full w-full rounded-full opacity-75"
            style={{
              background: color,
              animation: 'pulse-dot 2s ease-in-out infinite',
            }}
          />
        )}
        <span
          className="relative inline-flex rounded-full h-2.5 w-2.5"
          style={{ background: color }}
        />
      </div>
      <span className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>
        {label}
      </span>

      {/* Tooltip */}
      {errorMessage && (
        <div
          className="absolute hidden group-hover:block top-full mt-2 right-0 w-64 text-xs p-3 rounded-lg border shadow-xl z-50 pointer-events-none"
          style={{
            background: 'var(--bg-elevated)',
            borderColor: 'var(--border-default)',
            color: 'var(--text-primary)',
          }}
        >
          {errorMessage}
        </div>
      )}
    </div>
  );
}
