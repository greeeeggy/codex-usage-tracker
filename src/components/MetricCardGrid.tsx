import { MetricCard } from './MetricCard';
import { Clock, MonitorDot, Zap, Timer } from 'lucide-react';
import { TokenTotals } from '../types/usage';
import { formatTokensAsDuration } from '../utils/cn';

interface MetricCardGridProps {
  tokenTotals: TokenTotals | null;
}

/** Mini bar visualization (sparkline-like) */
function MiniBarViz({ heights, color }: { heights: number[]; color: string }) {
  return (
    <>
      {heights.map((h, i) => (
        <div
          key={i}
          className="rounded-sm"
          style={{
            width: 3,
            height: h,
            background: color,
            opacity: 0.5 + (i / heights.length) * 0.5,
          }}
        />
      ))}
    </>
  );
}

export function MetricCardGrid({ tokenTotals }: MetricCardGridProps) {
  // Today's usage: derive from tokenTotals.today if available
  const todayTokens = tokenTotals?.today.totalTokens ?? null;
  const todayValue = todayTokens !== null && todayTokens > 0 ? formatTokensAsDuration(todayTokens) : null;

  return (
    <div className="grid grid-cols-2 gap-4">
      <MetricCard
        icon={<Clock size={15} />}
        label="Today's Usage"
        value={todayValue}
        subtext={todayTokens !== null && todayTokens > 0 ? `${(todayTokens).toLocaleString()} tokens` : undefined}
        visualization={
          todayValue ? <MiniBarViz heights={[6, 10, 8, 14, 12, 16, 14]} color="var(--purple)" /> : undefined
        }
      />

      <MetricCard
        icon={<MonitorDot size={15} />}
        label="Sessions"
        value={null}
        subtext={undefined}
        visualization={undefined}
      />

      <MetricCard
        icon={<Zap size={15} />}
        label="Peak Activity"
        value={null}
        subtext={undefined}
        visualization={undefined}
      />

      <MetricCard
        icon={<Timer size={15} />}
        label="Longest Session"
        value={null}
        subtext={undefined}
        visualization={undefined}
      />
    </div>
  );
}
