import { TokenTotals, UsageDeltas } from '../types/usage';
interface MetricCardGridProps { tokenTotals: TokenTotals | null; usageDeltas: UsageDeltas | null }

export function MetricCardGrid({ usageDeltas }: MetricCardGridProps) {
  const items = [
    ['Sessions today', usageDeltas ? String(usageDeltas.sessionsToday) : '—'],
    ['Peak quota · last hour', usageDeltas ? usageDeltas.peakHourUsed.toFixed(1) + '%' : '—'],
    ['Longest session today', usageDeltas ? usageDeltas.longestSessionMinutes + ' min' : '—'],
  ];
  return <section className="monitor-panel">
    <h2>Monitoring activity</h2>
    <dl>{items.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
    <p>Based on recorded monitoring sessions.</p>
  </section>;
}
