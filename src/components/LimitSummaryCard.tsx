import { UsageWindow } from '../types/usage';
import { CalendarDays } from 'lucide-react';

interface LimitSummaryCardProps {
  weeklyWindow: UsageWindow | undefined;
}

export function LimitSummaryCard({ weeklyWindow }: LimitSummaryCardProps) {
  if (!weeklyWindow) {
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
        <div className="flex items-center justify-center py-6">
          <span className="text-sm" style={{ color: 'var(--text-muted)' }}>
            No limit data available
          </span>
        </div>
      </div>
    );
  }

  const used = Math.round(weeklyWindow.usedPercent);
  const remaining = Math.round(weeklyWindow.remainingPercent);

  // Mini circular progress for the summary
  const miniSize = 52;
  const miniStroke = 5;
  const miniRadius = (miniSize - miniStroke) / 2;
  const miniCircumference = 2 * Math.PI * miniRadius;
  const miniOffset = miniCircumference - (remaining / 100) * miniCircumference;
  const center = miniSize / 2;

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

      <div className="flex items-center gap-5">
        {/* Summary rows */}
        <div className="flex-1 space-y-2.5">
          <div className="flex items-center gap-2">
            <div className="w-1.5 h-1.5 rounded-full" style={{ background: 'var(--text-muted)' }} />
            <span className="text-[12px] flex-1" style={{ color: 'var(--text-muted)' }}>Weekly Limit</span>
            <span className="text-[12px] font-medium tabular-nums" style={{ color: 'var(--text-primary)' }}>
              {weeklyWindow.name === 'weekly' ? '10080m' : `${weeklyWindow.durationMinutes ?? '—'}m`}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <div className="w-1.5 h-1.5 rounded-full" style={{ background: 'var(--purple)' }} />
            <span className="text-[12px] flex-1" style={{ color: 'var(--text-muted)' }}>Used</span>
            <span className="text-[12px] font-medium tabular-nums" style={{ color: 'var(--text-primary)' }}>
              {used}%
            </span>
          </div>
          <div className="flex items-center gap-2">
            <div className="w-1.5 h-1.5 rounded-full" style={{ background: 'var(--green)' }} />
            <span className="text-[12px] flex-1" style={{ color: 'var(--text-muted)' }}>Remaining</span>
            <span className="text-[12px] font-medium tabular-nums" style={{ color: 'var(--text-primary)' }}>
              {remaining}%
            </span>
          </div>
        </div>

        {/* Mini circular progress */}
        <div className="shrink-0 relative" style={{ width: miniSize, height: miniSize }}>
          <svg width={miniSize} height={miniSize} className="transform -rotate-90">
            <circle
              cx={center}
              cy={center}
              r={miniRadius}
              fill="transparent"
              stroke="rgba(255,255,255,0.06)"
              strokeWidth={miniStroke}
            />
            <circle
              cx={center}
              cy={center}
              r={miniRadius}
              fill="transparent"
              stroke="var(--purple)"
              strokeWidth={miniStroke}
              strokeDasharray={miniCircumference}
              strokeDashoffset={miniOffset}
              strokeLinecap="round"
              style={{ transition: 'stroke-dashoffset 600ms ease-out' }}
            />
          </svg>
          <div className="absolute inset-0 flex items-center justify-center">
            <CalendarDays size={16} style={{ color: 'var(--text-muted)' }} />
          </div>
        </div>
      </div>
    </div>
  );
}
