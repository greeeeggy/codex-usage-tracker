import { useState, useEffect } from 'react';

interface CountdownProps {
  resetsAt: string | null | undefined;
  /** Current used percentage — when 0, the countdown hasn't truly started yet */
  usedPercent?: number;
  /** Duration of this window in minutes (10080 = 7 days, 300 = 5 hours) */
  durationMinutes?: number | null;
  className?: string;
  fallback?: string;
}

/**
 * Format a full-duration string from a window's durationMinutes.
 * e.g., 10080 → "7d 0h 0m", 300 → "5h 0m"
 */
function formatFullDuration(durationMinutes: number | null | undefined): string {
  if (!durationMinutes) return '7d 0h 0m'; // default to weekly
  const totalMs = durationMinutes * 60_000;
  const d = Math.floor(totalMs / 86400000);
  const h = Math.floor((totalMs % 86400000) / 3600000);
  const m = Math.floor((totalMs % 3600000) / 60000);
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m 0s`;
}

export function Countdown({
  resetsAt,
  usedPercent,
  durationMinutes,
  className = '',
  fallback = '—',
}: CountdownProps) {
  const [timeLeft, setTimeLeft] = useState<string>('');

  // If usage hasn't started (0%), show the full static duration instead of a live countdown.
  const usageStarted = usedPercent !== undefined && usedPercent !== null && usedPercent > 0;

  useEffect(() => {
    // No usage yet → show full static duration (e.g. "7d 0h 0m")
    if (!usageStarted) {
      setTimeLeft(formatFullDuration(durationMinutes));
      return;
    }

    if (!resetsAt) {
      setTimeLeft(fallback);
      return;
    }

    const resetTime = new Date(resetsAt).getTime();

    const updateTimer = () => {
      const now = Date.now();
      const diff = resetTime - now;

      if (diff <= 0) {
        setTimeLeft('Resetting...');
        return;
      }

      const d = Math.floor(diff / 86400000);
      const h = Math.floor((diff % 86400000) / 3600000);
      const m = Math.floor((diff % 3600000) / 60000);
      const s = Math.floor((diff % 60000) / 1000);

      if (d > 0) {
        setTimeLeft(`${d}d ${h}h ${m}m`);
      } else if (h > 0) {
        setTimeLeft(`${h}h ${m}m`);
      } else {
        setTimeLeft(`${m}m ${s}s`);
      }
    };

    updateTimer();
    const interval = setInterval(updateTimer, 1000);
    return () => clearInterval(interval);
  }, [resetsAt, fallback, usageStarted, durationMinutes]);

  return (
    <span className={`tabular-nums ${className}`} style={{ fontFamily: 'var(--font-sans)' }}>
      {timeLeft || fallback}
    </span>
  );
}
