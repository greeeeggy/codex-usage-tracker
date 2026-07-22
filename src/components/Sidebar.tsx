import { useUsageStore, type AppPage } from '../stores/usageStore';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  LayoutDashboard,
  BarChart3,
  History,
  Gauge,
  MonitorDot,
  Lightbulb,
  Settings,
  Crown,
  ChevronRight,
} from 'lucide-react';

interface NavItem {
  id: AppPage;
  label: string;
  icon: React.ElementType;
}

const navItems: NavItem[] = [
  { id: 'overview', label: 'Overview', icon: LayoutDashboard },
  { id: 'usage', label: 'Usage', icon: BarChart3 },
  { id: 'history', label: 'History', icon: History },
  { id: 'limits', label: 'Limits', icon: Gauge },
  { id: 'sessions', label: 'Sessions', icon: MonitorDot },
  { id: 'insights', label: 'Insights', icon: Lightbulb },
  { id: 'settings', label: 'Settings', icon: Settings },
];

export function Sidebar() {
  const activePage = useUsageStore((s) => s.activePage);
  const setActivePage = useUsageStore((s) => s.setActivePage);
  const snapshot = useUsageStore((s) => s.snapshot);

  const planType = snapshot?.planType;

  return (
    <aside
      className="flex flex-col shrink-0 h-full select-none"
      style={{
        width: 'var(--sidebar-width)',
        background: 'var(--bg-sidebar)',
        borderRight: '1px solid var(--border-default)',
      }}
    >
      {/* Logo area */}
      <div
        className="flex items-center gap-2.5 px-5 shrink-0"
        style={{ height: 'var(--titlebar-height)' }}
        data-tauri-drag-region
      >
        <div
          className="w-7 h-7 rounded-lg flex items-center justify-center"
          style={{ background: 'var(--purple-dim)', border: '1px solid var(--border-emphasized)' }}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--purple)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
          </svg>
        </div>
        <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
          Codex Meter
        </span>
      </div>

      {/* Navigation */}
      <nav className="flex-1 px-3 py-4 space-y-1 overflow-y-auto">
        {navItems.map((item) => {
          const isActive = activePage === item.id;
          const Icon = item.icon;

          return (
            <button
              key={item.id}
              onClick={() => setActivePage(item.id)}
              className="w-full flex items-center gap-3 px-3 rounded-lg transition-all duration-150 text-left group cursor-pointer"
              style={{
                height: '42px',
                background: isActive ? 'var(--purple-dim)' : 'transparent',
                border: isActive ? '1px solid rgba(139, 92, 246, 0.3)' : '1px solid transparent',
                color: isActive ? 'var(--text-primary)' : 'var(--text-secondary)',
              }}
              onMouseEnter={(e) => {
                if (!isActive) {
                  e.currentTarget.style.background = 'rgba(255,255,255,0.05)';
                  e.currentTarget.style.color = 'var(--text-primary)';
                }
              }}
              onMouseLeave={(e) => {
                if (!isActive) {
                  e.currentTarget.style.background = 'transparent';
                  e.currentTarget.style.color = 'var(--text-secondary)';
                }
              }}
              aria-current={isActive ? 'page' : undefined}
              title={item.label}
            >
              <Icon
                size={18}
                style={{
                  color: isActive ? 'var(--purple-bright)' : 'var(--text-muted)',
                  transition: 'color 150ms ease',
                }}
              />
              <span className="text-[13px] font-medium">{item.label}</span>
            </button>
          );
        })}
      </nav>

      {/* Plan card at bottom */}
      {planType && (
        <div className="mx-3 mb-4 p-4 rounded-xl" style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}>
          <div className="flex items-center gap-2 mb-2">
            <Crown size={14} style={{ color: 'var(--purple)' }} />
            <span className="text-xs font-bold tracking-wider uppercase" style={{ color: 'var(--purple-bright)' }}>
              {planType} Plan
            </span>
          </div>
          <p className="text-[11px] mb-3" style={{ color: 'var(--text-muted)', lineHeight: '1.4' }}>
            Advanced tracking and unlimited history.
          </p>
          <button
            className="flex items-center gap-1 text-xs font-medium px-3 py-1.5 rounded-lg transition-colors duration-150 cursor-pointer"
            style={{
              background: 'var(--purple-dim)',
              color: 'var(--purple-bright)',
              border: '1px solid rgba(139, 92, 246, 0.2)',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = 'var(--purple-glow)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = 'var(--purple-dim)';
            }}
            onClick={() => openUrl('https://chatgpt.com/#settings/Billing')}
          >
            Manage Plan <ChevronRight size={12} />
          </button>
        </div>
      )}
    </aside>
  );
}
