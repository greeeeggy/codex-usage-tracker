import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { useUsageStore } from '../stores/usageStore';
import { AccountDayPage } from '../types/usage';

export function AccountUsageCoverage() {
  const account = useUsageStore(s => s.accountUsage);
  const snapshot = useUsageStore(s => s.snapshot);
  const [page, setPage] = useState<AccountDayPage>({ days: [], total: 0 });
  const [offset, setOffset] = useState(0);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    void invoke<AccountDayPage>('get_account_usage_days', { offset })
      .then(data => { if (!disposed && data) { setPage(data); setError(null); } })
      .catch(e => { if (!disposed) setError(String(e)); });
    return () => { disposed = true; };
  }, [account, offset]);
  const fetched = account?.fetchedAt;
  return (
    <section className="rounded-2xl p-5 space-y-3" aria-label="Account-wide usage coverage"
      style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
      <h3 className="font-semibold" style={{ color: 'var(--text-primary)' }}>Shared account usage</h3>
      <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
        Quota monitoring covers the shared Work/Codex allowance, including web, cloud, other devices, and connected apps using that allowance.
        Monitoring continues while this app is running, even with no local Codex window open. Ordinary Chat conversations and separately billed API usage are not collected.
      </p>
      <div className="flex flex-wrap gap-6 text-sm" style={{ color: 'var(--text-primary)' }}>
        <span>Server lifetime tokens: <strong>{account?.summary?.lifetimeTokens?.toLocaleString() ?? 'Unavailable'}</strong></span>
        <span>Quota observed: {snapshot?.windows.length ? new Date(snapshot.capturedAt).toLocaleString() : 'Waiting for account data'}</span>
      </div>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        {fetched ? `Account activity last fetched ${new Date(fetched * 1000).toLocaleString()}. ` : 'Account token activity is unavailable until the service returns it. '}
        Server totals are kept separately from local logs to avoid double counting. The service does not provide exact remote tokens for each limit period;
        daily totals cannot be split accurately across five-hour windows or model buckets. Missed observations while this app is closed cannot be reconstructed.
      </p>
      {error && <p role="alert" className="text-xs" style={{ color: 'var(--danger)' }}>{error}</p>}
      <details>
        <summary className="text-sm cursor-pointer" style={{ color: 'var(--text-secondary)' }}>{page.total} saved days · Server-reported tokens</summary>
        <div className="overflow-auto max-h-72 mt-3">
          <table className="w-full text-sm text-left" aria-label="Server-reported daily tokens" style={{ color: 'var(--text-primary)' }}>
            <thead><tr>{['Reported day', 'Account tokens', 'Last observed'].map(h => <th key={h} className="p-2">{h}</th>)}</tr></thead>
            <tbody>{page.days.map(day => <tr key={day.startDate} className="border-t border-white/5">
              <td className="p-2">{day.startDate}</td><td className="p-2 font-mono">{day.tokens.toLocaleString()}</td>
              <td className="p-2">{new Date(day.observedAt * 1000).toLocaleString()}</td>
            </tr>)}</tbody>
          </table>
          {!page.total && <p className="p-3 text-xs" style={{ color: 'var(--text-muted)' }}>No server day totals recorded yet.</p>}
        </div>
        <div className="flex justify-between mt-3 text-xs" style={{ color: 'var(--text-muted)' }}>
          <span>{page.total ? `${offset + 1}–${Math.min(offset + 100, page.total)} of ${page.total} days` : '0 days'}</span>
          <div className="flex gap-3">
            <button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 100))} className="disabled:opacity-30 cursor-pointer">Previous</button>
            <button disabled={offset + 100 >= page.total} onClick={() => setOffset(offset + 100)} className="disabled:opacity-30 cursor-pointer">Next</button>
          </div>
        </div>
      </details>
    </section>
  );
}
