import { useUsageStore } from '../stores/usageStore';

export function AccountSelector() {
  const accounts = useUsageStore(s => s.accounts);
  const active = useUsageStore(s => s.activeAccount);
  const selected = useUsageStore(s => s.selectedAccountKey);
  const select = useUsageStore(s => s.selectAccount);
  return (
    <select aria-label="Account" value={selected ?? ''} onChange={e => select(e.target.value || null)}
      className="max-w-[280px] rounded-lg px-3 py-2 text-sm"
      style={{ background: 'var(--bg-elevated)', color: 'var(--text-primary)', border: '1px solid var(--border-default)' }}>
      <option value="">{active ? 'Signed-in account · ' + active.label : 'No account signed in'}</option>
      {accounts.map(account => {
        const duplicate = account.email && accounts.filter(a => a.email === account.email).length > 1;
        const suffix = duplicate ? ' · ' + account.accountKey.slice(-8) : '';
        return <option key={account.accountKey} value={account.accountKey}>
          {account.label + suffix + (account.accountKey === active?.accountKey ? ' · signed in' : '')}
        </option>;
      })}
    </select>
  );
}
