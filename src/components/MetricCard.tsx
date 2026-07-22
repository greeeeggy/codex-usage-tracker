import { ReactNode } from 'react';

interface MetricCardProps {
  icon: ReactNode;
  label: string;
  value: string | null;
  subtext?: string;
  /** Optional small visualization element */
  visualization?: ReactNode;
}

export function MetricCard({ icon, label, value, subtext, visualization }: MetricCardProps) {
  return (
    <div
      className="rounded-xl p-4 transition-colors duration-200 flex flex-col justify-between"
      style={{
        background: 'var(--bg-card)',
        border: '1px solid var(--border-default)',
        minHeight: 120,
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = 'var(--bg-card-hover)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = 'var(--bg-card)';
      }}
    >
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <span style={{ color: 'var(--text-muted)' }}>{icon}</span>
          <span className="text-[12px] font-medium" style={{ color: 'var(--text-muted)' }}>
            {label}
          </span>
        </div>
        {visualization && (
          <div className="flex items-end gap-[2px]">{visualization}</div>
        )}
      </div>

      {value !== null ? (
        <div>
          <span
            className="text-xl font-semibold tabular-nums block"
            style={{ color: 'var(--text-primary)' }}
          >
            {value}
          </span>
          {subtext && (
            <span className="text-[11px] block mt-0.5" style={{ color: 'var(--text-muted)' }}>
              {subtext}
            </span>
          )}
        </div>
      ) : (
        <span className="text-sm" style={{ color: 'var(--text-faint)' }}>
          No data
        </span>
      )}
    </div>
  );
}
