import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';

interface WeeklyUsageChartProps {
  data: { timestamp: number; value: number }[];
  onViewHistory?: () => void;
}

export function WeeklyUsageChart({ data, onViewHistory }: WeeklyUsageChartProps) {
  const single = data.length === 1;
  return (
    <section className="chart-panel" aria-label="Weekly quota observations">
      <div className="chart-heading"><h2>Weekly quota</h2><span>Observed usage · % of allowance</span></div>
      {data.length ? <ResponsiveContainer width="100%" height={180}>
        <LineChart data={data} margin={{ top: 12, right: 10, left: 0, bottom: 0 }}>
          <CartesianGrid stroke="var(--border-subtle)" vertical={false} />
          <XAxis dataKey="timestamp" type="number" scale="time"
            domain={single ? [data[0].timestamp - 1_800_000, data[0].timestamp + 1_800_000] : ['dataMin', 'dataMax']}
            minTickGap={36} tickLine={false} axisLine={false}
            tick={{ fill: 'var(--text-muted)', fontSize: 10 }}
            tickFormatter={value => new Date(value).toLocaleString([], { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })} />
          <YAxis domain={[0, 100]} ticks={[0, 25, 50, 75, 100]} width={42} tickLine={false} axisLine={false}
            tick={{ fill: 'var(--text-muted)', fontSize: 10 }} tickFormatter={value => value + '%'} />
          <Tooltip labelFormatter={value => new Date(Number(value)).toLocaleString()}
            formatter={value => [Number(value).toFixed(1) + '%', 'Quota used']}
            contentStyle={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', borderRadius: 4, fontSize: 12 }}
            itemStyle={{ color: 'var(--text-primary)' }} labelStyle={{ color: 'var(--text-secondary)' }} />
          <Line dataKey="value" type="linear" stroke="var(--accent)" strokeWidth={2}
            dot={single ? { r: 3 } : false} activeDot={{ r: 3 }} isAnimationActive={false} />
        </LineChart>
      </ResponsiveContainer> : <div className="chart-empty"><p>No quota observations yet</p><span>History builds while Meter is running.</span></div>}
      {onViewHistory && <button className="text-link chart-footer" onClick={onViewHistory}>View history →</button>}
    </section>
  );
}
