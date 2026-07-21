import { TokenBreakdown } from '../types/usage';

interface TokenBreakdownCardProps {
  breakdown: TokenBreakdown | null;
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

export function TokenBreakdownCard({ breakdown }: TokenBreakdownCardProps) {
  if (!breakdown) {
    return null;
  }

  const items = [
    { label: 'Input', value: breakdown.inputTokens },
    { label: 'Cached input', value: breakdown.cachedInputTokens },
    { label: 'Uncached input', value: breakdown.uncachedInputTokens },
    { label: 'Output', value: breakdown.outputTokens },
  ];

  if (breakdown.reasoningTokens !== null && breakdown.reasoningTokens > 0) {
    items.push({ label: 'Reasoning', value: breakdown.reasoningTokens });
  }

  return (
    <div className="bg-white/5 border border-white/10 rounded-2xl p-6 backdrop-blur-md">
      <h3 className="text-sm font-semibold text-white/60 uppercase tracking-wider mb-4">Token Breakdown</h3>
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
