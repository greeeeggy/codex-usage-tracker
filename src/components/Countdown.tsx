import { useState, useEffect } from 'react';
import { cn } from '../utils/cn';

interface CountdownProps {
  resetsAt: string | null;
  className?: string;
}

export function Countdown({ resetsAt, className }: CountdownProps) {
  const [timeLeft, setTimeLeft] = useState<string>('');

  useEffect(() => {
    if (!resetsAt) {
      setTimeLeft('');
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
  }, [resetsAt]);

  if (!resetsAt) return null;

  return (
    <span className={cn('tabular-nums font-mono', className)}>
      {timeLeft}
    </span>
  );
}
