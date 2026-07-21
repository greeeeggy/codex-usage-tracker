import { cn, getRemainingColor, getRemainingColorText } from '../utils/cn';

interface UsageBarProps {
  label: string;
  remainingPercent: number;
  compact?: boolean;
}

export function UsageBar({ label, remainingPercent, compact = false }: UsageBarProps) {
  const bgColor = getRemainingColor(remainingPercent);
  const textColor = getRemainingColorText(remainingPercent);

  return (
    <div className={cn('flex items-center gap-3 w-full', compact ? 'mb-2' : 'mb-4')}>
      <span className={cn('flex-shrink-0 text-white/70 font-medium', compact ? 'text-xs w-14' : 'text-sm w-20')}>
        {label}
      </span>
      <div className={cn('flex-1 bg-white/10 rounded-full overflow-hidden', compact ? 'h-2' : 'h-3')}>
        <div
          className={cn('h-full rounded-full transition-all duration-1000 ease-out', bgColor)}
          style={{ width: `${Math.round(remainingPercent)}%` }}
        />
      </div>
      <span className={cn('flex-shrink-0 font-bold text-right tabular-nums', compact ? 'text-xs w-10' : 'text-sm w-12', textColor)}>
        {Math.round(remainingPercent)}%
      </span>
    </div>
  );
}
