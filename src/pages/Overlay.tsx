import { useEffect, useState } from 'react';
import { useUsageStore } from '../stores/usageStore';
import { Countdown } from '../components/Countdown';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { Pin, Minus, X, Clock } from 'lucide-react';

export function Overlay() {
  const init = useUsageStore((state) => state.init);
  const monitorState = useUsageStore((state) => state.monitorState);
  const snapshot = useUsageStore((state) => state.snapshot);
  const [isAlwaysOnTop, setIsAlwaysOnTop] = useState(true);

  useEffect(() => {
    init();
  }, [init]);

  const weeklyWindow = snapshot?.windows.find(
    (w) => w.name === 'weekly' || w.durationMinutes === 10080
  ) || snapshot?.windows[0];

  const remainingPercent = weeklyWindow ? Math.round(weeklyWindow.remainingPercent) : 0;

  const handleToggleAlwaysOnTop = async () => {
    try {
      const appWindow = getCurrentWindow();
      const next = !isAlwaysOnTop;
      await appWindow.setAlwaysOnTop(next);
      setIsAlwaysOnTop(next);
    } catch (e) {
      console.error('Failed to toggle always on top:', e);
    }
  };

  const handleMinimize = async () => {
    try {
      const appWindow = getCurrentWindow();
      await appWindow.minimize();
    } catch (e) {
      console.error('Failed to minimize widget:', e);
    }
  };

  const handleClose = async () => {
    await invoke('toggle_widget');
  };

  const handleOpenDashboard = async () => {
    await invoke('show_dashboard');
  };

  return (
    <div
      className="w-full h-full rounded-2xl p-4 flex flex-col justify-between select-none overflow-hidden"
      style={{
        background: 'rgba(17, 19, 31, 0.92)',
        backdropFilter: 'blur(16px)',
        border: '1px solid var(--border-default)',
        boxShadow: '0 8px 32px rgba(0, 0, 0, 0.4)',
        color: 'var(--text-primary)',
      }}
      data-tauri-drag-region
    >
      {/* Top Header Row */}
      <div className="flex items-center justify-between" data-tauri-drag-region>
        <div
          className="flex items-center gap-2 cursor-pointer group"
          onClick={handleOpenDashboard}
          title="Click to open dashboard"
        >
          <div
            className="w-5 h-5 rounded-md flex items-center justify-center"
            style={{ background: 'var(--purple-dim)' }}
          >
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="var(--purple)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
            </svg>
          </div>
          <span className="text-xs font-semibold group-hover:text-purple-300 transition-colors" style={{ color: 'var(--text-primary)' }}>
            Codex Meter
          </span>
        </div>

        {/* Action Controls */}
        <div className="flex items-center gap-1">
          <button
            onClick={handleToggleAlwaysOnTop}
            className="p-1 rounded transition-colors duration-150 cursor-pointer"
            style={{
              color: isAlwaysOnTop ? 'var(--purple-bright)' : 'var(--text-muted)',
            }}
            title={isAlwaysOnTop ? 'Always on top (active)' : 'Pin window'}
          >
            <Pin size={13} className={isAlwaysOnTop ? 'fill-purple-500/30' : ''} />
          </button>
          <button
            onClick={handleMinimize}
            className="p-1 rounded transition-colors duration-150 cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
            onMouseEnter={(e) => { e.currentTarget.style.color = 'var(--text-primary)'; }}
            onMouseLeave={(e) => { e.currentTarget.style.color = 'var(--text-muted)'; }}
            title="Minimize widget"
          >
            <Minus size={13} />
          </button>
          <button
            onClick={handleClose}
            className="p-1 rounded transition-colors duration-150 cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
            onMouseEnter={(e) => { e.currentTarget.style.color = 'var(--danger)'; }}
            onMouseLeave={(e) => { e.currentTarget.style.color = 'var(--text-muted)'; }}
            title="Hide widget"
          >
            <X size={14} />
          </button>
        </div>
      </div>

      {/* Middle Section */}
      <div className="my-1">
        {/* Label & Status */}
        <div className="flex items-center justify-between mb-2">
          <span className="text-xs font-semibold" style={{ color: 'var(--text-secondary)' }}>
            Week
          </span>
          <div className="flex items-center gap-1.5">
            <span
              className="w-2 h-2 rounded-full"
              style={{
                background: monitorState === 'monitoring' ? 'var(--green)' : 'var(--text-muted)',
                boxShadow: monitorState === 'monitoring' ? '0 0 8px var(--green)' : 'none',
              }}
            />
            <span className="text-[11px] font-medium" style={{ color: 'var(--text-secondary)' }}>
              {monitorState === 'monitoring' ? 'Monitoring' : 'Dormant'}
            </span>
          </div>
        </div>

        {/* Progress Bar & Percentage */}
        <div className="flex items-center gap-3">
          <div
            className="flex-1 rounded-full overflow-hidden"
            style={{ height: 10, background: 'rgba(255, 255, 255, 0.08)' }}
          >
            <div
              className="h-full rounded-full transition-all duration-700 ease-out"
              style={{
                width: `${remainingPercent}%`,
                background: 'linear-gradient(90deg, var(--purple) 0%, var(--blue) 100%)',
              }}
            />
          </div>
          <span
            className="text-xl font-bold tabular-nums shrink-0"
            style={{ color: 'var(--purple-bright)' }}
          >
            {remainingPercent}%
          </span>
        </div>
      </div>

      {/* Bottom Row: Reset Countdown */}
      <div className="flex items-center gap-1.5 text-[11px]" style={{ color: 'var(--text-muted)' }}>
        <Clock size={12} />
        {weeklyWindow?.resetsAt ? (
          <span>
            Resets in <Countdown resetsAt={weeklyWindow.resetsAt} usedPercent={weeklyWindow.usedPercent} durationMinutes={weeklyWindow.durationMinutes} className="font-semibold text-white/90" />
          </span>
        ) : (
          <span>No reset schedule</span>
        )}
      </div>
    </div>
  );
}
