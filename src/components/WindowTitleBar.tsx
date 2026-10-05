import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Minus, Square, X, Copy } from 'lucide-react';

export function WindowTitleBar() {
  const [isMaximized, setIsMaximized] = useState(false);
  const appWindow = getCurrentWindow();

  useEffect(() => {
    let disposed = false;
    const update = async () => {
      const maximized = await appWindow.isMaximized();
      if (!disposed) setIsMaximized(maximized);
    };
    void update();
    const unlisten = appWindow.onResized(() => { void update(); });
    return () => { disposed = true; void unlisten.then(fn => fn()); };
  }, []);

  return (
    <div className="window-titlebar" data-tauri-drag-region role="toolbar" aria-label="Window controls">
      <div className="window-brand" data-tauri-drag-region>
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" style={{ pointerEvents: 'none' }}>
          <path d="M2 12V8m6 4V3m6 9V6" stroke="var(--accent)" strokeWidth="2" strokeLinecap="square" />
        </svg>
        <span data-tauri-drag-region>Codex Meter</span>
      </div>
      <div className="window-buttons">
        <button onClick={() => appWindow.minimize()} aria-label="Minimize window" title="Minimize"><Minus size={14} /></button>
        <button onClick={async () => {
          await appWindow.toggleMaximize();
          setIsMaximized(await appWindow.isMaximized());
        }} aria-label={isMaximized ? 'Restore window' : 'Maximize window'} title={isMaximized ? 'Restore' : 'Maximize'}>
          {isMaximized ? <Copy size={12} /> : <Square size={12} />}
        </button>
        <button className="window-close" onClick={() => appWindow.hide()} aria-label="Close window" title="Close to tray"><X size={15} /></button>
      </div>
    </div>
  );
}
