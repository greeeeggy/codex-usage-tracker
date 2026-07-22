import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from 'recharts';
import { ChevronRight } from 'lucide-react';

interface ChartDataPoint {
  timestamp: string;
  value: number;
}

interface WeeklyUsageChartProps {
  data: ChartDataPoint[];
}

export function WeeklyUsageChart({ data }: WeeklyUsageChartProps) {
  const hasData = data.length > 0;

  return (
    <div
      className="rounded-xl p-5 flex flex-col"
      style={{
        background: 'var(--bg-card)',
        border: '1px solid var(--border-default)',
        minHeight: 260,
      }}
    >
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <span className="text-[15px] font-semibold" style={{ color: 'var(--text-primary)' }}>
            Usage This Week
          </span>
          {hasData && (
            <div className="flex items-center gap-1.5">
              <div className="w-2 h-2 rounded-full" style={{ background: 'var(--purple)' }} />
              <span className="text-[11px]" style={{ color: 'var(--text-muted)' }}>Hours</span>
            </div>
          )}
        </div>
      </div>

      {/* Chart or empty state */}
      {hasData ? (
        <div className="flex-1" style={{ minHeight: 180 }}>
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={data} margin={{ top: 5, right: 10, left: -20, bottom: 0 }}>
              <defs>
                <linearGradient id="usageGradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="var(--purple)" stopOpacity={0.25} />
                  <stop offset="95%" stopColor="var(--purple)" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid
                strokeDasharray="3 3"
                stroke="rgba(255,255,255,0.04)"
                vertical={false}
              />
              <XAxis
                dataKey="timestamp"
                stroke="transparent"
                tick={{ fill: 'var(--text-muted)', fontSize: 11 }}
                tickLine={false}
                axisLine={false}
              />
              <YAxis
                stroke="transparent"
                tick={{ fill: 'var(--text-muted)', fontSize: 11 }}
                tickLine={false}
                axisLine={false}
                width={40}
              />
              <Tooltip
                contentStyle={{
                  backgroundColor: 'var(--bg-elevated)',
                  border: '1px solid var(--border-default)',
                  borderRadius: '8px',
                  fontSize: '12px',
                }}
                itemStyle={{ color: 'var(--text-primary)' }}
                labelStyle={{ color: 'var(--text-muted)' }}
              />
              <Area
                type="monotone"
                dataKey="value"
                stroke="var(--purple)"
                strokeWidth={2}
                fillOpacity={1}
                fill="url(#usageGradient)"
                dot={{ r: 3, fill: 'var(--purple)', strokeWidth: 0 }}
                activeDot={{ r: 5, fill: 'var(--purple-bright)', strokeWidth: 2, stroke: 'var(--bg-card)' }}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      ) : (
        <div
          className="flex-1 flex flex-col items-center justify-center rounded-lg"
          style={{ background: 'rgba(255,255,255,0.02)' }}
        >
          <span className="text-sm mb-1" style={{ color: 'var(--text-muted)' }}>
            No usage data this week
          </span>
          <span className="text-[11px]" style={{ color: 'var(--text-faint)' }}>
            Usage history will appear after your first monitored session
          </span>
        </div>
      )}

      {/* Footer link */}
      <button
        className="flex items-center gap-1 text-[12px] font-medium mt-3 self-start transition-colors duration-150"
        style={{ color: 'var(--text-muted)' }}
        onMouseEnter={(e) => { e.currentTarget.style.color = 'var(--text-primary)'; }}
        onMouseLeave={(e) => { e.currentTarget.style.color = 'var(--text-muted)'; }}
      >
        View full history <ChevronRight size={12} />
      </button>
    </div>
  );
}
