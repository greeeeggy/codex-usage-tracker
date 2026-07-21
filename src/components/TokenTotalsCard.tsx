import { TokenTotals } from '../types/usage';

interface TokenTotalsCardProps {
  totals: TokenTotals | null;
}

const formatNumber = (num: number) => {
  if (num >= 1000000) {
    return (num / 1000000).toFixed(1) + 'M';
  }
  if (num >= 1000) {
    return (num / 1000).toFixed(1) + 'K';
  }
  return num.toString();
};

export function TokenTotalsCard({ totals }: TokenTotalsCardProps) {
  if (!totals) {
    return null;
  }

  const items = [
    { label: 'Current Session', value: totals.currentSession.totalTokens },
    { label: 'Current 5h Window', value: totals.fiveHourWindow.totalTokens },
    { label: 'Current Week', value: totals.weeklyWindow.totalTokens },
    { label: 'Today', value: totals.today.totalTokens },
    { label: 'This Month', value: totals.currentMonth.totalTokens },
    { label: 'All-Time Recorded', value: totals.allTimeRecorded.totalTokens },
  ];

  return (
    <div className="bg-white/5 border border-white/10 rounded-2xl p-6 backdrop-blur-md">
      <h3 className="text-sm font-semibold text-white/60 uppercase tracking-wider mb-4">Token Totals</h3>
      <div className="space-y-3">
        {items.map((item, idx) => (
          <div key={idx} className="flex justify-between items-center py-2 border-b border-white/5 last:border-0">
            <span className="text-white/80">{item.label}</span>
            <span className="font-mono font-semibold text-white">{formatNumber(item.value)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
