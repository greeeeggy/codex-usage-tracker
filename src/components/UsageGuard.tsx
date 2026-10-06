import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ShieldCheck, Copy, Check } from 'lucide-react';
interface Guard { status: 'ready' | 'pause' | 'unavailable'; shouldPause: boolean; remainingPercent?: number; resetsAt?: string; resumeAt?: string; prompt?: string | null; reason?: string }
export function UsageGuard() {
  const [guard, setGuard] = useState<Guard | null>(null);
  const [installed, setInstalled] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  useEffect(() => {
    let disposed = false;
    const load = async () => {
      try {
        const [g, integration] = await Promise.all([invoke<Guard>('get_usage_guard'), invoke<{ installed: boolean }>('get_guard_integration')]);
        if (!disposed) { setGuard(g); setInstalled(integration.installed); }
      } catch (e) { if (!disposed) setError(String(e)); }
    };
    void load(); const timer = window.setInterval(() => void load(), 5000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, []);
  const configure = async (enabled: boolean) => {
    setBusy(true); setError(''); setMessage('');
    try { const result = await invoke<{ installed: boolean }>('configure_usage_guard', { enabled }); setInstalled(result.installed); setMessage(enabled ? 'Connection installed. Restart Codex and review/trust the Codex Meter hooks once in Codex’s hook settings (/hooks in the CLI).' : 'Meter connection removed. Restart Codex to apply.'); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const copy = async (text: string) => { try { await navigator.clipboard.writeText(text); setMessage('Copied.'); } catch { setError('Could not copy. Select the text below to copy it manually.'); } };
  return <div className="feature-page">
    <section className="feature-intro"><ShieldCheck size={22} /><div><h2>Pause before the allowance runs out</h2><p>At 5% remaining, ask the AI to save its progress and schedule this chat to continue two minutes after the exact reset.</p></div></section>
    <section className="feature-panel" aria-label="Live quota guard"><div className="section-heading"><h3>Live guard</h3><span>{guard?.status === 'pause' ? 'Checkpoint needed' : guard?.status === 'ready' ? 'Allowance available' : 'Waiting for fresh usage'}</span></div>
      <div className="guard-readings"><div><span>Five-hour remaining</span><strong>{guard?.remainingPercent !== undefined ? `${guard.remainingPercent.toFixed(1)}%` : '—'}</strong></div><div><span>Pause threshold</span><strong>5%</strong></div><div><span>Resume buffer</span><strong>2 minutes</strong></div></div>
      {guard?.resumeAt && <p>Continue at <b>{new Date(guard.resumeAt).toLocaleString()}</b><br /><small>Exact time: {guard.resumeAt}</small></p>}
      {guard?.reason && <p>{guard.reason}</p>}
    </section>
    <section className="feature-panel"><h3>Connect Codex to Meter</h3><p>The connection adds a usage tool the AI can call and lifecycle hooks that supply the checkpoint prompt between tool calls and at the end of a turn. Keep Meter running.</p>
      <div className="feature-form"><button className="feature-button primary" disabled={busy} onClick={() => void configure(true)}>{installed ? <Check size={15} /> : <ShieldCheck size={15} />}{installed ? 'Update connection' : 'Enable Usage Guard'}</button>{installed && <button className="feature-button" disabled={busy} onClick={() => void configure(false)}>Remove connection</button>}</div>
      <p className="feature-footnote">After enabling, restart Codex and review/trust the new hooks once. Supported local Codex/Work chats can use this connection. Cloud ChatGPT chats need a separate reachable connector. Continuation is scheduled by the AI using the app’s scheduling tool; if that tool is unavailable, the prompt asks it to report the resume time. Hooks check at tool boundaries, so they cannot interrupt an already-running command.</p>
    </section>
    {error && <p role="alert" className="feature-error">{error}</p>}{message && <p role="status" className="feature-notice">{message}</p>}
    <section className="feature-panel"><div className="section-heading"><h3>Local API</h3><button className="feature-button" onClick={() => void copy('http://127.0.0.1:32145/api/guard')}><Copy size={14} /> Copy address</button></div><code className="feature-code">GET http://127.0.0.1:32145/api/guard</code><p>Returns freshness, remaining percentage, pause status, the exact resume timestamp, and the ready-to-use checkpoint prompt. It exposes no login tokens and cannot switch accounts.</p></section>
    {guard?.prompt && <section className="feature-panel"><div className="section-heading"><h3>Checkpoint prompt</h3><button className="feature-button" onClick={() => void copy(guard.prompt!)}><Copy size={14} /> Copy prompt</button></div><p className="guard-prompt">{guard.prompt}</p></section>}
  </div>;
}
