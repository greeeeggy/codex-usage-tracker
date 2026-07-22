import { useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Minus, Square, X, Copy } from 'lucide-react';

export function WindowTitleBar() {
  const [isMaximized, setIsMaximized] = useState(false);

  const appWindow = getCurrentWindow();

  const handleMinimize = () => appWindow.minimize();
  const handleToggleMaximize = async () => {
    await appWindow.toggleMaximize();
    setIsMaximized(await appWindow.isMaximized());
  };
  const handleClose = () => appWindow.hide();

  return (
    <div
      className="flex items-center justify-between shrink-0 select-none"
      style={{
        height: 'var(--titlebar-height)',
        background: 'var(--bg-titlebar)',
        borderBottom: '1px solid var(--border-default)',
      }}
      data-tauri-drag-region
    >
      {/* Left: App identity */}
      <div className="flex items-center gap-2 pl-4 pointer-events-none">
        <div
          className="w-5 h-5 rounded-md flex items-center justify-center"
          style={{ background: 'var(--purple-dim)' }}
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="var(--purple)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
          </svg>
        </div>
        <span
          className="text-xs font-semibold tracking-wide"
          style={{ color: 'var(--text-secondary)' }}
        >
          Codex Meter
        </span>
      </div>

      {/* Right: Window controls */}
      <div className="flex items-center h-full">
        <button
          onClick={handleMinimize}
          className="h-full px-3.5 flex items-center justify-center transition-colors duration-150"
          style={{ color: 'var(--text-muted)' }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = 'rgba(255,255,255,0.08)';
            e.currentTarget.style.color = 'var(--text-primary)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'transparent';
            e.currentTarget.style.color = 'var(--text-muted)';
          }}
          aria-label="Minimize window"
          title="Minimize"
        >
          <Minus size={14} />
        </button>
        <button
          onClick={handleToggleMaximize}
          className="h-full px-3.5 flex items-center justify-center transition-colors duration-150"
          style={{ color: 'var(--text-muted)' }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = 'rgba(255,255,255,0.08)';
            e.currentTarget.style.color = 'var(--text-primary)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'transparent';
            e.currentTarget.style.color = 'var(--text-muted)';
          }}
          aria-label={isMaximized ? 'Restore window' : 'Maximize window'}
          title={isMaximized ? 'Restore' : 'Maximize'}
        >
          {isMaximized ? <Copy size={12} /> : <Square size={12} />}
        </button>
        <button
          onClick={handleClose}
          className="h-full px-3.5 flex items-center justify-center transition-colors duration-150"
          style={{ color: 'var(--text-muted)' }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = 'rgba(255, 77, 94, 0.85)';
            e.currentTarget.style.color = '#fff';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'transparent';
            e.currentTarget.style.color = 'var(--text-muted)';
          }}
          aria-label="Close window"
          title="Close"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
