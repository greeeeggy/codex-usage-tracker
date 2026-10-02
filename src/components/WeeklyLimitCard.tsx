import { UsageWindow } from '../types/usage';
import { CircularProgress } from './CircularProgress';
import { UsageProgressBar } from './UsageProgressBar';
import { Countdown } from './Countdown';
import { CalendarDays, TrendingDown, TrendingUp, Timer } from 'lucide-react';
import { format } from 'date-fns';

interface WeeklyLimitCardProps {
  window: UsageWindow | undefined;
  compact?: boolean;
}

export function WeeklyLimitCard({ window, compact = false }: WeeklyLimitCardProps) {
  if (!window) {
    return (
      <div
        className="rounded-2xl p-8 flex items-center justify-center"
        style={{
          background: 'var(--bg-card)',
          border: '1px solid var(--border-default)',
          minHeight: 200,
        }}
      >
        <span style={{ color: 'var(--text-muted)' }}>No limit data available</span>
      </div>
    );
  }

  const used = Math.round(window.usedPercent);
  const remaining = Math.round(window.remainingPercent);

  let resetDateStr = '';
  if (window.resetsAt) {
    try {
      resetDateStr = format(new Date(window.resetsAt), "EEEE, MMMM d, yyyy h:mm a");
    } catch {
      resetDateStr = window.resetsAt;
    }
  }

  const limitTitle = window.name === 'fiveHour' ? '5-Hour Limit'
    : window.name === 'weekly' ? 'Weekly Limit'
    : window.durationMinutes ? `${window.durationMinutes}-Minute Limit` : 'Usage Limit';
  const valueClass = compact
    ? 'text-xl font-semibold tabular-nums'
    : 'text-2xl font-semibold tabular-nums';

  return (
    <div
      className={compact
        ? 'rounded-2xl p-5 transition-colors duration-200'
        : 'rounded-2xl p-6 transition-colors duration-200'}
      style={{
        background: 'var(--bg-card)',
        border: '1px solid var(--border-default)',
      }}
    >
      {/* Card header */}
      <div className={compact ? 'flex items-center gap-2 mb-4' : 'flex items-center gap-2 mb-6'}>
        <CalendarDays size={16} style={{ color: 'var(--text-muted)' }} />
        <span className="text-[15px] font-semibold" style={{ color: 'var(--text-primary)' }}>
          {limitTitle}
        </span>
      </div>

      {/* Main content row */}
      <div className={compact ? 'flex items-center gap-5 flex-wrap' : 'flex items-center gap-8 flex-wrap'}>
        {/* Circular progress */}
        <div className="shrink-0">
          <CircularProgress
            value={remaining}
            size={compact ? 112 : 150}
            strokeWidth={compact ? 8 : 10}
          />
        </div>

        {/* Metrics columns */}
        <div className={compact
          ? 'flex gap-5 flex-wrap flex-1 min-w-0'
          : 'flex gap-10 flex-wrap flex-1 min-w-0'}>
          {/* Used */}
          <div className="flex flex-col">
            <div className="flex items-center gap-1.5 mb-1">
              <div className="w-2 h-2 rounded-full" style={{ background: 'var(--purple)' }} />
              <span className="text-xs font-medium" style={{ color: 'var(--text-muted)' }}>Used</span>
            </div>
            <span className={valueClass} style={{ color: 'var(--text-primary)' }}>
              {used}%
            </span>
            <div className="flex items-center gap-1 mt-1" style={{ color: 'var(--text-muted)' }}>
              <TrendingUp size={12} />
            </div>
          </div>

          {/* Remaining */}
          <div className="flex flex-col">
            <div className="flex items-center gap-1.5 mb-1">
              <div className="w-2 h-2 rounded-full" style={{ background: 'var(--green)' }} />
              <span className="text-xs font-medium" style={{ color: 'var(--text-muted)' }}>Remaining</span>
            </div>
            <span className={valueClass} style={{ color: 'var(--text-primary)' }}>
              {remaining}%
            </span>
            <div className="flex items-center gap-1 mt-1" style={{ color: 'var(--text-muted)' }}>
              <TrendingDown size={12} />
            </div>
          </div>

          {/* Reset In */}
          <div className="flex flex-col">
            <div className="flex items-center gap-1.5 mb-1">
              <div className="w-2 h-2 rounded-full" style={{ background: 'var(--blue)' }} />
              <span className="text-xs font-medium" style={{ color: 'var(--text-muted)' }}>Reset in</span>
            </div>
            <span className={valueClass} style={{ color: 'var(--text-primary)' }}>
              <Countdown resetsAt={window.resetsAt} usedPercent={window.usedPercent} durationMinutes={window.durationMinutes} />
            </span>
            <div className="flex items-center gap-1 mt-1" style={{ color: 'var(--text-muted)' }}>
              <Timer size={12} />
            </div>
          </div>
        </div>
      </div>

      {/* Progress bar */}
      <div className={compact ? 'mt-4' : 'mt-6'}>
        <UsageProgressBar value={remaining} />
      </div>

      {/* Reset date */}
      {resetDateStr && used > 0 && (
        <p className="text-[11px] mt-2" style={{ color: 'var(--text-muted)' }}>
          {limitTitle} resets on {resetDateStr}
        </p>
      )}
    </div>
  );
}
