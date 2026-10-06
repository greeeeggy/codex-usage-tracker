import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ArrowLeftRight, Plus, LockKeyhole, Trash2 } from 'lucide-react';
import { useUsageStore } from '../stores/usageStore';
import type { AccountProfile } from '../types/usage';

interface SavedLogin { id: string; label: string; account: AccountProfile; savedAt: string }
interface SwitchStatus { profiles: SavedLogin[]; activeAccount: AccountProfile | null; loginPending: boolean }
export function SwitchAccounts() {
  const [status, setStatus] = useState<SwitchStatus>({ profiles: [], activeAccount: null, loginPending: false });
  const [label, setLabel] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const liveAccount = useUsageStore(s => s.activeAccount);
  const selectAccount = useUsageStore(s => s.selectAccount);
  const reload = async () => setStatus(await invoke<SwitchStatus>('get_switch_accounts'));
  useEffect(() => { void reload().catch(e => setError(String(e))); }, [liveAccount?.accountKey]);
  useEffect(() => {
    const unlisten = listen<string>('switch-progress', e => setMessage(e.payload));
    return () => { void unlisten.then(fn => fn()); };
  }, []);
  useEffect(() => {
    if (!status.loginPending) return;
    let disposed = false;
    let inFlight = false;
    const timer = window.setInterval(async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const saved = await invoke<SavedLogin | null>('poll_account_login');
        if (saved && !disposed) { setMessage(`${saved.label} added. Choose Switch to use this account.`); setLabel(''); await reload(); }
      } catch (e) { if (!disposed) { setError(String(e)); await reload().catch(() => {}); } }
      finally { inFlight = false; }
    }, 1500);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [status.loginPending]);
  const run = async (action: () => Promise<unknown>, success: string) => {
    setBusy(true); setError(''); setMessage('');
    try { await action(); setMessage(success); await reload(); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const active = liveAccount?.accountKey ?? status.activeAccount?.accountKey;
  return <div className="feature-page">
    <section className="feature-intro">
      <ArrowLeftRight size={22} />
      <div><h2>Your accounts, one click away</h2><p>Save each account once. Switching closes Codex normally and reopens it with the account you choose.</p></div>
    </section>
    <section className="feature-panel" aria-label="Add a Codex account">
      <h3>Add an account</h3>
      <p>Save the account already signed into Codex, or sign into another one. A new account needs a browser sign-in once; later switches use its saved login.</p>
      <div className="feature-form">
        <label htmlFor="account-name">Account name</label>
        <input id="account-name" placeholder="Personal, Work, Account B…" value={label} onChange={e => setLabel(e.target.value)} maxLength={100} disabled={busy || status.loginPending} />
        <button className="feature-button" disabled={busy || status.loginPending || !active} onClick={() => void run(() => invoke('save_current_login', { label }), 'Current login saved.')}><LockKeyhole size={15} /> Save current account</button>
        <button className="feature-button primary" disabled={busy || status.loginPending || !label.trim()} onClick={() => void run(() => invoke('start_account_login', { label }), 'Complete the browser sign-in. Your current Codex account stays active.')}><Plus size={15} /> Add another account</button>
      </div>
      {status.loginPending && <div className="feature-notice"><span>Waiting for browser sign-in…</span><button className="feature-button" disabled={busy} onClick={() => void run(() => invoke('cancel_account_login'), 'Sign-in cancelled.')}>Cancel sign-in</button></div>}
    </section>
    {error && <p role="alert" className="feature-error">{error}</p>}
    {message && <p role="status" className="feature-notice">{message}</p>}
    <section className="feature-panel" aria-label="Saved logins">
      <div className="section-heading"><h3>Saved accounts</h3><span>{status.profiles.length} saved</span></div>
      {status.profiles.length === 0 && <p>No saved logins yet. Save your current account to get started.</p>}
      <div className="saved-login-list">{status.profiles.map(p => <article key={p.id} className="saved-login">
        <div><h4>{p.label}{active === p.account.accountKey && <span className="active-login">Active</span>}</h4><p>{p.account.email ?? p.account.label}{p.account.planType ? ` · ${p.account.planType}` : ''}</p><small>Saved {new Date(p.savedAt).toLocaleString()}</small></div>
        <div className="saved-login-actions"><button className="feature-button primary" disabled={busy || status.loginPending || active === p.account.accountKey} aria-label={`Switch to ${p.label}`} onClick={() => void run(async () => { setMessage('Closing Codex and activating the selected account…'); await invoke('switch_codex_account', { id: p.id }); selectAccount(null); }, `Codex relaunched with ${p.label}. Usage is refreshing.`)}><ArrowLeftRight size={15} /> {active === p.account.accountKey ? 'Current account' : 'Switch'}</button><button className="icon-button" disabled={busy || status.loginPending} aria-label={`Remove saved login ${p.label}`} title="Remove saved login; keeps recorded history" onClick={() => void run(() => invoke('remove_saved_login', { id: p.id }), 'Saved login removed. Usage history is retained.')}><Trash2 size={15} /></button></div>
      </article>)}</div>
    </section>
    <p className="feature-footnote"><LockKeyhole size={14} /> Saved logins are encrypted for your Windows user. Finish active Codex work before switching. Chats and workspace files stay in place; CLI and editor sessions are not restarted.</p>
  </div>;
}
