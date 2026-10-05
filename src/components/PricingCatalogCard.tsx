import { invoke } from '@tauri-apps/api/core';
import { useState } from 'react';
import { useUsageStore } from '../stores/usageStore';

export function PricingCatalogCard() {
  const pricing = useUsageStore(s => s.pricing);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const stale = !pricing?.fetchedAt || Date.now() / 1000 - pricing.fetchedAt >= 6 * 3600;
  return (
    <section className="rounded-2xl p-5 space-y-3" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
      <div className="flex justify-between gap-4">
        <h3 className="font-semibold" style={{ color: 'var(--text-primary)' }}>Model pricing</h3>
        <button disabled={refreshing} onClick={async () => {
          setRefreshing(true); setError(null);
          try { await invoke('refresh_pricing'); } catch (e) { setError(String(e)); } finally { setRefreshing(false); }
        }} className="text-sm cursor-pointer disabled:opacity-50" style={{ color: 'var(--purple-bright)' }}>{refreshing ? 'Updating…' : 'Update prices'}</button>
      </div>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Prices update automatically from <a href="https://developers.openai.com/api/docs/pricing" target="_blank" rel="noreferrer" className="underline">OpenAI’s pricing page</a> every six hours.
        New listed models are discovered automatically. Saved prices remain available offline.
        Estimates use standard API rates; subscription quota is separate.
      </p>
      <p className="text-xs" style={{ color: stale ? 'var(--warning)' : 'var(--text-secondary)' }}>
        {pricing?.fetchedAt ? `Last updated: ${new Date(pricing.fetchedAt * 1000).toLocaleString()}${stale ? ' · Saved prices may be out of date' : ''}` : 'Waiting for the first pricing update'}
      </p>
      {(error || pricing?.lastError) && <p role="alert" className="text-xs" style={{ color: 'var(--danger)' }}>Pricing update failed: {error ?? pricing?.lastError}. Saved prices are retained.</p>}
      <details>
        <summary className="text-sm cursor-pointer" style={{ color: 'var(--text-secondary)' }}>{Object.keys(pricing?.models ?? {}).length} models · View rates per million tokens</summary>
        <div className="overflow-auto max-h-80 mt-3">
          <table className="w-full text-sm text-left" style={{ color: 'var(--text-primary)' }}>
            <thead><tr>{['Model', 'Input', 'Cached', 'Cache write', 'Output'].map(h => <th key={h} className="p-2">{h}</th>)}</tr></thead>
            <tbody>{Object.entries(pricing?.models ?? {}).map(([model, price]) => <tr key={model} className="border-t border-white/5">
              <td className="p-2">{model}</td>{[price.standard.input, price.standard.cachedInput, price.standard.cacheWrite, price.standard.output].map((rate, i) => <td key={i} className="p-2 font-mono">{rate === null ? '—' : `$${rate}`}</td>)}
            </tr>)}</tbody>
          </table>
        </div>
      </details>
    </section>
  );
}
