import { UsageWindow } from '../types/usage';
import { Countdown } from './Countdown';

interface WeeklyLimitCardProps { window: UsageWindow | undefined; compact?: boolean }

export function WeeklyLimitCard({ window, compact = false }: WeeklyLimitCardProps) {
  const title = !window ? 'Quota' : window.name === 'fiveHour' ? '5-hour quota' : window.name === 'weekly' ? 'Weekly quota' : window.durationMinutes ? window.durationMinutes + '-minute quota' : 'Quota';
  if (!window) return <section className="quota-panel"><h2>{title}</h2><p className="quota-empty">Waiting for quota data</p></section>;
  const remaining = Math.max(0, Math.min(100, window.remainingPercent));
  const reset = window.resetsAt ? new Date(window.resetsAt) : null;
  const validReset = reset && Number.isFinite(reset.getTime());
  const color = remaining <= 0 ? 'var(--danger)' : remaining <= 25 ? 'var(--warning)' : 'var(--accent)';
  return (
    <section className={'quota-panel' + (compact ? ' quota-compact' : '')} aria-label={title}>
      <h2>{title}</h2>
      <div className="quota-reading">
        <div><strong>{Math.round(remaining)}<span>%</span></strong><span className="quota-label">remaining</span></div>
        <div className="quota-reset"><span className="quota-label">Resets in</span>
          <b><Countdown resetsAt={window.resetsAt} usedPercent={window.usedPercent} durationMinutes={window.durationMinutes} /></b>
        </div>
      </div>
      <div className="quota-track" role="progressbar" aria-label={title + ' remaining'} aria-valuemin={0} aria-valuemax={100} aria-valuenow={remaining}>
        <div style={{ width: remaining + '%', background: color }} />
      </div>
      <div className="quota-footnote">
        <span>{Math.round(window.usedPercent)}% used</span>
        <span title={validReset ? reset.toLocaleString() : undefined}>
          {validReset && window.usedPercent > 0 ? 'Resets ' + reset.toLocaleString([], { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' }) : window.usedPercent === 0 ? 'Window starts with first use' : 'Reset time unavailable'}
        </span>
      </div>
    </section>
  );
}
