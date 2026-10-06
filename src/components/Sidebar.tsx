import { useUsageStore, type AppPage } from '../stores/usageStore';
import { openUrl } from '@tauri-apps/plugin-opener';
import { LayoutDashboard, BarChart3, History, Gauge, MonitorDot, Lightbulb, Settings, ArrowUpRight, ArrowLeftRight, ShieldCheck } from 'lucide-react';

const items = [
  { id: 'overview', label: 'Overview', icon: LayoutDashboard },
  { id: 'switch', label: 'Switch', icon: ArrowLeftRight },
  { id: 'guard', label: 'Usage Guard', icon: ShieldCheck },
  { id: 'usage', label: 'Usage', icon: BarChart3 },
  { id: 'history', label: 'History', icon: History },
  { id: 'limits', label: 'Limits', icon: Gauge },
  { id: 'sessions', label: 'Sessions', icon: MonitorDot },
  { id: 'insights', label: 'Insights', icon: Lightbulb },
  { id: 'settings', label: 'Settings', icon: Settings },
] as const;

export function Sidebar() {
  const active = useUsageStore(s => s.activePage);
  const select = useUsageStore(s => s.setActivePage);
  const plan = useUsageStore(s => s.snapshot?.planType);
  return (
    <aside className="sidebar">
      <nav aria-label="Dashboard pages">
        {items.map(({ id, label, icon: Icon }) => (
          <button key={id} onClick={() => select(id as AppPage)} aria-current={active === id ? 'page' : undefined}>
            <Icon size={16} strokeWidth={1.6} /><span>{label}</span>
          </button>
        ))}
      </nav>
      <div className="sidebar-footer">
        {plan && <>
          <span className="plan-label">ChatGPT <span className="capitalize">{plan}</span></span>
          <button className="text-link" onClick={() => openUrl('https://chatgpt.com/#settings/Billing')}>Manage plan <ArrowUpRight size={12} /></button>
        </>}
        <span className="tray-hint">Close to keep running in tray</span>
      </div>
    </aside>
  );
}
