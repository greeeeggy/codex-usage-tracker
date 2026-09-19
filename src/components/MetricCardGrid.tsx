import { MetricCard } from './MetricCard';
import { Clock, MonitorDot, Zap, Timer } from 'lucide-react';
import { TokenTotals, UsageDeltas } from '../types/usage';
import { formatNumber } from '../utils/cn';

interface MetricCardGridProps {
  tokenTotals: TokenTotals | null;
  usageDeltas: UsageDeltas | null;
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

export function MetricCardGrid({ tokenTotals, usageDeltas }: MetricCardGridProps) {
  // Today's usage: derive from tokenTotals.today if available
  const todayTokens = tokenTotals?.today.totalTokens ?? null;
  const todayValue = todayTokens !== null && todayTokens > 0 
    ? formatNumber(todayTokens) 
    : (usageDeltas?.todayDelta ? `+${usageDeltas.todayDelta.toFixed(1)}%` : null);

  const subtext = todayTokens !== null && todayTokens > 0 
    ? `${todayTokens.toLocaleString()} total tokens` 
    : (usageDeltas?.todayDelta ? `Quota used today` : undefined);

  return (
    <div className="grid grid-cols-2 gap-4">
      <MetricCard
        icon={<Clock size={15} />}
        label={todayTokens !== null && todayTokens > 0 ? "Today's Tokens" : "Today's Usage"}
        value={todayValue}
        subtext={subtext}
        visualization={
          todayValue ? <MiniBarViz heights={[6, 10, 8, 14, 12, 16, 14]} color="var(--purple)" /> : undefined
        }
      />

      <MetricCard
        icon={<MonitorDot size={15} />}
        label="Sessions"
        value={usageDeltas?.sessionsToday?.toString() || null}
        subtext={usageDeltas?.sessionsToday ? 'Sessions today' : undefined}
        visualization={undefined}
      />

      <MetricCard
        icon={<Zap size={15} />}
        label="Peak Activity"
        value={usageDeltas?.peakHourUsed ? `${usageDeltas.peakHourUsed.toFixed(1)}%` : null}
        subtext={usageDeltas?.peakHourUsed ? 'Peak usage per hour' : undefined}
        visualization={undefined}
      />

      <MetricCard
        icon={<Timer size={15} />}
        label="Longest Session"
        value={usageDeltas?.longestSessionMinutes ? `${usageDeltas.longestSessionMinutes}m` : null}
        subtext={usageDeltas?.longestSessionMinutes ? 'Longest session today' : undefined}
        visualization={undefined}
      />
    </div>
  );
}
