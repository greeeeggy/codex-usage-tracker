import { useUsageStore } from '../stores/usageStore';
import { RefreshCw, Settings2 } from 'lucide-react';
import { AccountSelector } from './AccountSelector';

const pageTitles = { overview: 'Overview', usage: 'Usage', history: 'History', limits: 'Limits', sessions: 'Sessions', insights: 'Insights', settings: 'Settings' };

export function DashboardHeader() {
  const snapshot = useUsageStore(s => s.snapshot);
  const monitor = useUsageStore(s => s.monitorState);
  const error = useUsageStore(s => s.errorMessage);
  const refresh = useUsageStore(s => s.refresh);
  const refreshing = useUsageStore(s => s.isRefreshing);
  const page = useUsageStore(s => s.activePage);
  const navigate = useUsageStore(s => s.setActivePage);
  const saved = useUsageStore(s => s.selectedAccountKey !== null && s.selectedAccountKey !== s.activeAccount?.accountKey);
  const status = saved ? 'Saved history' : monitor === 'monitoring' ? 'Monitoring' : monitor === 'connecting' ? 'Connecting' : monitor === 'error' || monitor === 'authRequired' ? 'Connection issue' : 'Waiting';
  const color = saved ? 'var(--text-muted)' : monitor === 'monitoring' ? 'var(--green)' : monitor === 'error' || monitor === 'authRequired' ? 'var(--danger)' : 'var(--text-muted)';
  return (
    <header className="dashboard-header">
      <div className="page-heading">
        <h1>{pageTitles[page]}</h1>
        <span className="monitor-status" title={error ?? status}><i style={{ background: color }} />{status}</span>
      </div>
      <div className="dashboard-tools">
        <AccountSelector />
        {snapshot && <span className="refresh-time" title={new Date(snapshot.capturedAt).toLocaleString()}>
          {saved ? 'Observed ' : 'Updated '}{new Date(snapshot.capturedAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}
        </span>}
        <button className="icon-button" onClick={refresh} disabled={saved || refreshing} aria-label="Refresh usage data" title="Refresh">
          <RefreshCw size={15} style={refreshing ? { animation: 'spin 1s linear infinite' } : undefined} />
        </button>
        <button className="icon-button" onClick={() => navigate('settings')} aria-label="Open settings" title="Settings"><Settings2 size={15} /></button>
      </div>
    </header>
  );
}
