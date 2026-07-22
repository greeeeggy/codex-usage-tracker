interface UsageProgressBarProps {
  /** Used or remaining percentage 0-100 */
  usedPercent?: number;
  value?: number;
  className?: string;
}

export function UsageProgressBar({ usedPercent, value, className = '' }: UsageProgressBarProps) {
  const percent = value ?? usedPercent ?? 0;
  const clamped = Math.max(0, Math.min(100, Math.round(percent)));

  return (
    <div className={className}>
      {/* Bar */}
      <div
        className="relative w-full rounded-full overflow-hidden"
        style={{ height: 8, background: 'rgba(255,255,255,0.06)' }}
      >
        <div
          className="h-full rounded-full"
          style={{
            width: `${clamped}%`,
            background: 'linear-gradient(90deg, var(--purple), var(--blue))',
            transition: 'width 800ms ease-out',
          }}
        />
      </div>

      {/* Markers */}
      <div className="flex justify-between mt-1.5">
        <span className="text-[11px] tabular-nums" style={{ color: 'var(--text-muted)' }}>0%</span>
        <span className="text-[11px] tabular-nums" style={{ color: 'var(--text-muted)' }}>100%</span>
      </div>
    </div>
  );
}
