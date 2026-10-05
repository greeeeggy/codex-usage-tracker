import { useUsageStore } from '../stores/usageStore';

export function AccountSelector() {
  const accounts = useUsageStore(s => s.accounts);
  const active = useUsageStore(s => s.activeAccount);
  const selected = useUsageStore(s => s.selectedAccountKey);
  const select = useUsageStore(s => s.selectAccount);
  const profile = selected ? accounts.find(a => a.accountKey === selected) : active;
  return (
    <label className="account-selector">
      <span>Account</span>
      <select aria-label="Account" title={profile?.label ?? 'No account signed in'} value={selected ?? ''} onChange={e => select(e.target.value || null)}>
        <option value="">{active ? active.label + ' · live' : 'No account signed in'}</option>
        {accounts.map(account => {
          const duplicate = account.email && accounts.filter(a => a.email === account.email).length > 1;
          const suffix = duplicate ? ' · ' + account.accountKey.slice(-8) : '';
          return <option key={account.accountKey} value={account.accountKey}>
            {account.label + suffix + (account.accountKey === active?.accountKey ? ' · signed in' : '')}
          </option>;
        })}
      </select>
    </label>
  );
}
