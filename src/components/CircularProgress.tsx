interface CircularProgressProps {
  /** Value 0–100 */
  value: number;
  /** Overall size in px */
  size?: number;
  /** Stroke thickness in px */
  strokeWidth?: number;
  /** Show "Remaining" label */
  showLabel?: boolean;
  className?: string;
}

export function CircularProgress({
  value,
  size = 160,
  strokeWidth = 10,
  showLabel = true,
  className = '',
}: CircularProgressProps) {
  const clamped = Math.max(0, Math.min(100, Math.round(value)));
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (clamped / 100) * circumference;
  const center = size / 2;

  return (
    <div className={`relative inline-flex items-center justify-center ${className}`} style={{ width: size, height: size }}>
      <svg width={size} height={size} className="transform -rotate-90">
        {/* Background track */}
        <circle
          cx={center}
          cy={center}
          r={radius}
          fill="transparent"
          stroke="rgba(255,255,255,0.06)"
          strokeWidth={strokeWidth}
        />
        {/* Gradient definition */}
        <defs>
          <linearGradient id="progress-gradient" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="var(--purple)" />
            <stop offset="100%" stopColor="var(--blue)" />
          </linearGradient>
        </defs>
        {/* Progress arc */}
        <circle
          cx={center}
          cy={center}
          r={radius}
          fill="transparent"
          stroke="url(#progress-gradient)"
          strokeWidth={strokeWidth}
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          strokeLinecap="round"
          style={{ transition: 'stroke-dashoffset 800ms ease-out' }}
        />
      </svg>

      {/* Center text */}
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        <span
          className="font-semibold tabular-nums"
          style={{
            fontSize: size * 0.22,
            color: 'var(--text-primary)',
            lineHeight: 1.1,
          }}
        >
          {clamped}%
        </span>
        {showLabel && (
          <span
            className="font-medium"
            style={{
              fontSize: size * 0.085,
              color: 'var(--text-muted)',
              marginTop: 4,
            }}
          >
            Remaining
          </span>
        )}
      </div>
    </div>
  );
}
