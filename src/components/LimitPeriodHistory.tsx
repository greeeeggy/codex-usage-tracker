import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { useUsageStore } from '../stores/usageStore';
import { LimitHistoryPage } from '../types/usage';

const date = (ts: number) => new Date(ts * 1000).toLocaleString();
const count = (n: number) => n.toLocaleString();

export function LimitPeriodHistory() {
  const snapshot = useUsageStore(s => s.snapshot);
  const [page, setPage] = useState<LimitHistoryPage>({ periods: [], total: 0 });
  const [kind, setKind] = useState('');
  const [bucket, setBucket] = useState('');
  const [offset, setOffset] = useState(0);
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let disposed = false;
    const unlisteners: (() => void)[] = [];
    for (const event of ['limit-history-updated', 'usage-updated']) {
      void listen(event, () => setRevision(r => r + 1)).then(unlisten => {
        if (disposed) unlisten(); else unlisteners.push(unlisten);
      });
    }
    const timer = window.setInterval(() => setRevision(r => r + 1), 30_000);
    return () => { disposed = true; unlisteners.forEach(fn => fn()); window.clearInterval(timer); };
  }, []);

  useEffect(() => {
    let disposed = false;
    const timer = window.setTimeout(() => {
      setLoading(true);
      void invoke<LimitHistoryPage>('get_limit_history', { limitId: bucket || null, windowKind: kind || null, offset })
        .then(data => { if (!disposed) { setPage(data); setError(null); } })
        .catch(e => { if (!disposed) setError(String(e)); })
        .finally(() => { if (!disposed) setLoading(false); });
    }, 150);
    return () => { disposed = true; window.clearTimeout(timer); };
  }, [bucket, kind, offset, revision]);

  const ids = [...new Set([...(snapshot?.limits ?? []).map(b => b.limitId), ...page.periods.map(p => p.limitId)])];
  return (
    <section className="rounded-2xl p-5 space-y-4" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h3 className="font-semibold" style={{ color: 'var(--text-primary)' }}>Token history by limit period</h3>
        <div className="flex flex-wrap gap-3 text-sm">
          <input aria-label="Limit bucket" list="limit-buckets" placeholder="All limit buckets" value={bucket}
            onChange={e => { setBucket(e.target.value); setOffset(0); }} className="rounded-lg p-2"
            style={{ background: 'var(--bg-elevated)', color: 'var(--text-primary)' }} />
          <datalist id="limit-buckets">{ids.map(id => <option key={id} value={id} />)}</datalist>
          <select aria-label="Window duration" value={kind} onChange={e => { setKind(e.target.value); setOffset(0); }}
            className="rounded-lg p-2" style={{ background: 'var(--bg-elevated)', color: 'var(--text-primary)' }}>
            <option value="">All windows</option><option value="fiveHour">5-hour</option><option value="weekly">Weekly</option>
          </select>
        </div>
      </div>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        One summary per limit cycle. Completed cycles stay saved after resets and restarts. Total tokens cover requests with shared-limit attribution in this computer’s Codex logs.
        Account quota includes Work, Codex, web, cloud, and other clients sharing the allowance. Exact remote tokens per period are unavailable.
        Five-hour and weekly totals overlap; do not add them together.
      </p>
      {error && <p role="alert" style={{ color: 'var(--danger)' }}>{error}</p>}
      <div className="overflow-x-auto">
        <table className="w-full text-sm text-left" aria-label="Limit period token history" aria-busy={loading}>
          <thead style={{ color: 'var(--text-muted)' }}>
            <tr>{['Limit / window', 'Start', 'Reset', 'Total tokens', 'Observed quota consumed'].map(h => <th key={h} className="p-3 whitespace-nowrap">{h}</th>)}</tr>
          </thead>
          <tbody style={{ color: 'var(--text-primary)' }}>
            {page.periods.map(period => (
              <tr key={period.id} className="border-t border-white/5">
                <td className="p-3 whitespace-nowrap"><strong>{period.limitName ?? period.limitId}</strong><br />
                  <span style={{ color: 'var(--text-muted)' }}>{period.windowKind === 'fiveHour' ? '5-hour' : period.windowKind === 'weekly' ? 'Weekly' : period.windowKind}</span>
                  <span className="ml-2 text-xs capitalize" style={{ color: 'var(--text-muted)' }}>{period.status}</span>
                </td>
                <td className="p-3 whitespace-nowrap">{date(period.startedAt)}</td>
                <td className="p-3 whitespace-nowrap">{date(period.resetsAt)}</td>
                <td className="p-3 font-mono font-semibold" title={`Input: ${count(period.tokens.inputTokens)} · Cached input (included): ${count(period.tokens.cachedInputTokens)} · Output: ${count(period.tokens.outputTokens)} · Reasoning (included in output): ${count(period.tokens.reasoningTokens ?? 0)}`}>{count(period.tokens.totalTokens)}</td>
                <td className="p-3" title={`Last observed ${date(period.lastObservedAt)}`}>{period.usedPercent.toFixed(1)}%</td>
              </tr>
            ))}
            {!loading && !error && page.periods.length === 0 && <tr><td colSpan={5} className="p-8 text-center" style={{ color: 'var(--text-muted)' }}>No recorded periods match. Existing logs are imported automatically.</td></tr>}
          </tbody>
        </table>
      </div>
      <div className="flex items-center justify-between text-xs" style={{ color: 'var(--text-muted)' }}>
        <span>{page.total ? `${offset + 1}–${Math.min(offset + 100, page.total)} of ${page.total} periods` : '0 periods'} · {Intl.DateTimeFormat().resolvedOptions().timeZone}</span>
        <div className="flex gap-3">
          <button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 100))} className="disabled:opacity-30 cursor-pointer">Previous</button>
          <button disabled={offset + 100 >= page.total} onClick={() => setOffset(offset + 100)} className="disabled:opacity-30 cursor-pointer">Next</button>
        </div>
      </div>
    </section>
  );
}
