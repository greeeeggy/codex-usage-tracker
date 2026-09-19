import { ReactNode } from 'react';
import { CalendarDays, Clock3 } from 'lucide-react';
import { UsageWindow } from '../types/usage';
import { getRemainingColor } from '../utils/cn';

interface LimitSummaryCardProps {
  fiveHourWindow: UsageWindow | undefined;
  weeklyWindow: UsageWindow | undefined;
}

function LimitRow({
  label,
  window,
  icon,
}: {
  label: string;
  window: UsageWindow;
  icon: ReactNode;
}) {
  const used = Math.round(window.usedPercent);
  const remaining = Math.round(window.remainingPercent);
  const color = getRemainingColor(remaining);

  return (
    <div className="rounded-lg p-3" style={{ background: 'var(--bg-elevated)' }}>
      <div className="flex items-center gap-2 mb-2">
        <span style={{ color: 'var(--text-muted)' }}>{icon}</span>
        <span className="text-[12px] font-medium flex-1" style={{ color: 'var(--text-secondary)' }}>
          {label}
        </span>
        <span className="text-sm font-semibold tabular-nums" style={{ color }}>
          {remaining}% remaining
        </span>
      </div>
      <div
        className="h-1.5 rounded-full overflow-hidden"
        style={{ background: 'rgba(255,255,255,0.07)' }}
      >
        <div
          className="h-full rounded-full transition-all duration-500"
          style={{ width: `${remaining}%`, background: color }}
        />
      </div>
      <div
        className="flex justify-between mt-1.5 text-[10px]"
        style={{ color: 'var(--text-muted)' }}
      >
        <span>{used}% used</span>
        <span>
          {window.durationMinutes === 300
            ? '5 hours'
            : window.durationMinutes === 10_080
              ? '7 days'
              : `${window.durationMinutes ?? '—'} min`}
        </span>
      </div>
    </div>
  );
}

export function LimitSummaryCard({ fiveHourWindow, weeklyWindow }: LimitSummaryCardProps) {
  const hasLimits = Boolean(fiveHourWindow || weeklyWindow);

  return (
    <div
      className="rounded-xl p-5"
      style={{
        background: 'var(--bg-card)',
        border: '1px solid var(--border-default)',
      }}
    >
      <h3 className="text-[15px] font-semibold mb-4" style={{ color: 'var(--text-primary)' }}>
        Limit Summary
      </h3>

      {hasLimits ? (
        <div className="space-y-3">
          {fiveHourWindow && (
            <LimitRow
              label="5-Hour Limit"
              window={fiveHourWindow}
              icon={<Clock3 size={14} />}
            />
          )}
          {weeklyWindow && (
            <LimitRow
              label="Weekly Limit"
              window={weeklyWindow}
              icon={<CalendarDays size={14} />}
            />
          )}
        </div>
      ) : (
        <div className="flex items-center justify-center py-6">
          <span className="text-sm" style={{ color: 'var(--text-muted)' }}>
            No limit data available
          </span>
        </div>
      )}
    </div>
  );
}
