import { useEffect } from 'react';
import { useUsageStore } from '../stores/usageStore';
import { UsageBar } from '../components/UsageBar';
import { Countdown } from '../components/Countdown';
import { invoke } from '@tauri-apps/api/core';

export function Overlay() {
  const init = useUsageStore((state) => state.init);
  const monitorState = useUsageStore((state) => state.monitorState);
  const snapshot = useUsageStore((state) => state.snapshot);

  useEffect(() => {
    init();
  }, [init]);

  const fiveHour = snapshot?.windows.find((w) => w.name === 'fiveHour' || w.durationMinutes === 300);
  const weekly = snapshot?.windows.find((w) => w.name === 'weekly' || w.durationMinutes === 10080);

  const handleContextMenu = async (e: React.MouseEvent) => {
    e.preventDefault();
    // In a real app we might show a custom context menu here
    // For now we just provide basic drag functionality
  };

  const handleClose = async () => {
    await invoke('toggle_widget');
  };

  const handleOpenDashboard = async () => {
    await invoke('show_dashboard');
  };

  return (
    <div 
      className="w-full h-full bg-neutral-950/85 backdrop-blur-md border border-white/10 rounded-xl p-3 text-white flex flex-col justify-between overflow-hidden select-none"
      onContextMenu={handleContextMenu}
      data-tauri-drag-region
    >
      <div className="flex items-center justify-between mb-2" data-tauri-drag-region>
        <div className="flex items-center gap-1.5 pointer-events-none">
          <div className={`w-1.5 h-1.5 rounded-full ${monitorState === 'monitoring' ? 'bg-green-500 animate-pulse' : 'bg-white/30'}`} />
          <span className="text-[10px] font-bold tracking-widest text-white/50 uppercase">Codex</span>
        </div>
        <div className="flex gap-2">
          <button onClick={handleOpenDashboard} className="text-white/30 hover:text-white/80 text-xs">↗</button>
          <button onClick={handleClose} className="text-white/30 hover:text-white/80 text-xs">×</button>
        </div>
      </div>

      <div className="flex-1 pointer-events-none">
        {monitorState === 'error' ? (
          <div className="text-red-400 text-xs text-center mt-2">Connection Error</div>
        ) : !snapshot || snapshot.windows.length === 0 ? (
          <div className="text-white/40 text-xs text-center mt-2">Waiting for data...</div>
        ) : (
          <>
            {fiveHour && (
              <UsageBar label="5h" remainingPercent={fiveHour.remainingPercent} compact />
            )}
            {weekly && (
              <UsageBar label="Wk" remainingPercent={weekly.remainingPercent} compact />
            )}
            {/* Render any other windows that might exist */}
            {snapshot.windows
              .filter((w) => w.name !== 'fiveHour' && w.durationMinutes !== 300 && w.name !== 'weekly' && w.durationMinutes !== 10080)
              .map((w) => (
                <UsageBar key={w.name} label={w.name.slice(0, 4)} remainingPercent={w.remainingPercent} compact />
              ))
            }
          </>
        )}
      </div>

      <div className="mt-1 text-[10px] text-white/40 pointer-events-none">
        {(() => {
          // Show reset info for the first window that has a resetsAt
          const windowWithReset = fiveHour?.resetsAt ? fiveHour : weekly?.resetsAt ? weekly : snapshot?.windows.find((w) => w.resetsAt);
          if (windowWithReset?.resetsAt) {
            const label = windowWithReset.name === 'fiveHour' ? '5h' : windowWithReset.name === 'weekly' ? 'Wk' : windowWithReset.name;
            return <>{label} resets in <Countdown resetsAt={windowWithReset.resetsAt} /></>;
          }
          return snapshot ? 'No reset info' : null;
        })()}
      </div>
    </div>
  );
}
