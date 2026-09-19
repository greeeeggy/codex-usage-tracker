import { ReactNode, useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi';
import { invoke } from '@tauri-apps/api/core';
import { CalendarDays, Clock3, Minimize2, Minus, Pin, X } from 'lucide-react';
import { useUsageStore } from '../stores/usageStore';
import { Countdown } from '../components/Countdown';
import { UsageWindow } from '../types/usage';
import { getRemainingColor } from '../utils/cn';

type WidgetMode = 'full' | 'percentage';
type ResizeDirection =
  | 'East'
  | 'North'
  | 'NorthEast'
  | 'NorthWest'
  | 'South'
  | 'SouthEast'
  | 'SouthWest'
  | 'West';

const WIDGET_MODE_KEY = 'codex-meter-widget-mode';
const FULL_WIDGET_SIZE = new LogicalSize(300, 170);
const FULL_WIDGET_MIN_SIZE = new LogicalSize(240, 142);
const PERCENTAGE_WIDGET_SIZE = new LogicalSize(92, 58);
const PERCENTAGE_WIDGET_MIN_SIZE = new LogicalSize(72, 46);
const SCREEN_EDGE_SNAP_DISTANCE = 40;

function readWidgetMode(): WidgetMode {
  return localStorage.getItem(WIDGET_MODE_KEY) === 'percentage' ? 'percentage' : 'full';
}

async function resizeWidget(size: LogicalSize, minSize: LogicalSize) {
  const appWindow = getCurrentWindow();
  if (await appWindow.isMaximized()) {
    await appWindow.unmaximize();
  }

  const activeScreen = window.screen as Screen & {
    availLeft?: number;
    availTop?: number;
  };
  const availableLeft = activeScreen.availLeft ?? 0;
  const availableTop = activeScreen.availTop ?? 0;
  const availableRight = availableLeft + window.screen.availWidth;
  const availableBottom = availableTop + window.screen.availHeight;
  const previousX = window.screenX;
  const previousY = window.screenY;
  const previousWidth = window.outerWidth;
  const previousHeight = window.outerHeight;
  const wasRightAligned =
    Math.abs(previousX + previousWidth - availableRight) <= SCREEN_EDGE_SNAP_DISTANCE;
  const wasBottomAligned =
    Math.abs(previousY + previousHeight - availableBottom) <= SCREEN_EDGE_SNAP_DISTANCE;

  await appWindow.setMinSize(minSize);
  await appWindow.setSize(size);

  const preferredX = wasRightAligned ? availableRight - size.width : previousX;
  const preferredY = wasBottomAligned ? availableBottom - size.height : previousY;
  const nextX = Math.min(
    Math.max(preferredX, availableLeft),
    Math.max(availableLeft, availableRight - size.width),
  );
  const nextY = Math.min(
    Math.max(preferredY, availableTop),
    Math.max(availableTop, availableBottom - size.height),
  );

  if (nextX !== previousX || nextY !== previousY) {
    await appWindow.setPosition(new LogicalPosition(nextX, nextY));
  }
}

const RESIZE_HANDLES: Array<{
  direction: ResizeDirection;
  className: string;
  cursor: string;
}> = [
  { direction: 'North', className: 'top-0 left-4 right-4 h-2.5', cursor: 'ns-resize' },
  { direction: 'South', className: 'bottom-0 left-4 right-4 h-2.5', cursor: 'ns-resize' },
  { direction: 'West', className: 'left-0 top-4 bottom-4 w-2.5', cursor: 'ew-resize' },
  { direction: 'East', className: 'right-0 top-4 bottom-4 w-2.5', cursor: 'ew-resize' },
  { direction: 'NorthWest', className: 'top-0 left-0 w-4 h-4', cursor: 'nwse-resize' },
  { direction: 'NorthEast', className: 'top-0 right-0 w-4 h-4', cursor: 'nesw-resize' },
  { direction: 'SouthWest', className: 'bottom-0 left-0 w-4 h-4', cursor: 'nesw-resize' },
  { direction: 'SouthEast', className: 'bottom-0 right-0 w-4 h-4', cursor: 'nwse-resize' },
];

function ResizeHandles() {
  const startResize = (event: React.PointerEvent<HTMLDivElement>, direction: ResizeDirection) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    getCurrentWindow().startResizeDragging(direction).catch((error) => {
      console.error(`Failed to resize widget from ${direction}:`, error);
    });
  };

  return (
    <>
      {RESIZE_HANDLES.map((handle) => (
        <div
          key={handle.direction}
          aria-hidden="true"
          data-resize-handle
          className={`absolute z-50 ${handle.className}`}
          style={{ cursor: handle.cursor }}
          onPointerDown={(event) => startResize(event, handle.direction)}
        />
      ))}
    </>
  );
}

function startWidgetDrag(event: React.PointerEvent<HTMLDivElement>) {
  if (event.button !== 0) return;

  const target = event.target as HTMLElement;
  if (target.closest('button, a, input, [data-widget-action], [data-resize-handle]')) {
    return;
  }

  event.preventDefault();
  getCurrentWindow().startDragging().catch((error) => {
    console.error('Failed to drag widget:', error);
  });
}

function LimitRow({
  label,
  icon,
  window,
}: {
  label: string;
  icon: ReactNode;
  window: UsageWindow;
}) {
  const remaining = Math.round(window.remainingPercent);
  const color = getRemainingColor(remaining);

  return (
    <div className="min-w-0">
      <div className="flex items-center gap-1.5 mb-1">
        <span style={{ color: 'var(--text-muted)' }}>{icon}</span>
        <span className="text-[11px] font-semibold" style={{ color: 'var(--text-secondary)' }}>
          {label}
        </span>
        <span className="ml-auto text-[10px]" style={{ color: 'var(--text-muted)' }}>
          Reset{' '}
          <Countdown
            resetsAt={window.resetsAt}
            usedPercent={window.usedPercent}
            durationMinutes={window.durationMinutes}
            className="font-semibold"
          />
        </span>
      </div>
      <div className="flex items-center gap-2.5">
        <div
          className="flex-1 rounded-full overflow-hidden"
          style={{ height: 6, background: 'rgba(255, 255, 255, 0.1)' }}
        >
          <div
            className="h-full rounded-full transition-all duration-700 ease-out"
            style={{ width: `${remaining}%`, background: color }}
          />
        </div>
        <span
          className="text-base font-semibold tabular-nums shrink-0 w-11 text-right"
          style={{ color }}
        >
          {remaining}%
        </span>
      </div>
    </div>
  );
}

export function Overlay() {
  const init = useUsageStore((state) => state.init);
  const monitorState = useUsageStore((state) => state.monitorState);
  const snapshot = useUsageStore((state) => state.snapshot);
  const [isAlwaysOnTop, setIsAlwaysOnTop] = useState(true);
  const [widgetMode, setWidgetMode] = useState<WidgetMode>(readWidgetMode);

  useEffect(() => {
    init();
  }, [init]);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    appWindow.isAlwaysOnTop().then(setIsAlwaysOnTop).catch(() => {});
  }, []);

  useEffect(() => {
    const isPercentage = widgetMode === 'percentage';
    const size = isPercentage ? PERCENTAGE_WIDGET_SIZE : FULL_WIDGET_SIZE;
    const minSize = isPercentage ? PERCENTAGE_WIDGET_MIN_SIZE : FULL_WIDGET_MIN_SIZE;
    resizeWidget(size, minSize).catch((error) => {
      console.error('Failed to resize widget:', error);
    });
  }, [widgetMode]);

  const fiveHourWindow = snapshot?.windows.find(
    (window) => window.name === 'fiveHour' || window.durationMinutes === 300,
  );
  const weeklyWindow = snapshot?.windows.find(
    (window) => window.name === 'weekly' || window.durationMinutes === 10_080,
  );
  const compactWindow = [fiveHourWindow, weeklyWindow]
    .filter((window): window is UsageWindow => Boolean(window))
    .sort((a, b) => a.remainingPercent - b.remainingPercent)[0];
  const compactPercent = compactWindow ? Math.round(compactWindow.remainingPercent) : null;

  const changeWidgetMode = (mode: WidgetMode) => {
    localStorage.setItem(WIDGET_MODE_KEY, mode);
    setWidgetMode(mode);
  };

  const handleToggleAlwaysOnTop = async () => {
    try {
      const appWindow = getCurrentWindow();
      const next = !isAlwaysOnTop;
      await appWindow.setAlwaysOnTop(next);
      setIsAlwaysOnTop(next);
    } catch (error) {
      console.error('Failed to toggle always on top:', error);
    }
  };

  const handleMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (error) {
      console.error('Failed to minimize widget:', error);
    }
  };

  const handleClose = async () => {
    await invoke('toggle_widget');
  };

  const handleOpenDashboard = async () => {
    await invoke('show_dashboard');
  };

  if (widgetMode === 'percentage') {
    return (
      <div
        className="relative w-full h-full rounded-xl flex items-center justify-center select-none overflow-hidden"
        style={{
          background: 'rgba(10, 12, 20, 0.46)',
          backdropFilter: 'blur(18px) saturate(125%)',
          border: '1px solid rgba(255, 255, 255, 0.13)',
          boxShadow: '0 6px 24px rgba(0, 0, 0, 0.28)',
          color: compactWindow
            ? getRemainingColor(compactWindow.remainingPercent)
            : 'var(--text-muted)',
        }}
        onPointerDown={startWidgetDrag}
      >
        <button
          type="button"
          className="text-3xl font-normal tabular-nums cursor-pointer bg-transparent border-0"
          aria-label={`${compactPercent ?? 'No'} percent remaining. Show the full widget.`}
          title="Most constrained limit remaining — click for full widget"
          onClick={() => changeWidgetMode('full')}
        >
          {compactPercent ?? '—'}%
        </button>
        <ResizeHandles />
      </div>
    );
  }

  return (
    <div
      className="relative w-full h-full rounded-xl px-3 py-2.5 flex flex-col select-none overflow-hidden"
      style={{
        background: 'rgba(10, 12, 20, 0.55)',
        backdropFilter: 'blur(18px) saturate(125%)',
        border: '1px solid rgba(255, 255, 255, 0.13)',
        boxShadow: '0 6px 24px rgba(0, 0, 0, 0.28)',
        color: 'var(--text-primary)',
      }}
      onPointerDown={startWidgetDrag}
    >
      <div className="flex items-center justify-between">
        <div
          className="flex items-center gap-2 cursor-pointer group"
          data-widget-action
          onClick={handleOpenDashboard}
          title="Open dashboard"
        >
          <div
            className="w-5 h-5 rounded-md flex items-center justify-center"
            style={{ background: 'var(--purple-dim)' }}
          >
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="var(--purple)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
            </svg>
          </div>
          <span className="text-xs font-semibold" style={{ color: 'var(--text-primary)' }}>
            Codex Meter
          </span>
          <span
            className="w-1.5 h-1.5 rounded-full"
            style={{
              background: monitorState === 'monitoring' ? 'var(--green)' : 'var(--text-muted)',
              boxShadow: monitorState === 'monitoring' ? '0 0 7px var(--green)' : 'none',
            }}
            title={monitorState === 'monitoring' ? 'Monitoring' : 'Dormant'}
          />
        </div>

        <div className="flex items-center gap-1">
          <button
            onClick={() => changeWidgetMode('percentage')}
            className="p-1 rounded cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
            title="Percentage-only mode (shows the most constrained limit)"
            aria-label="Switch to percentage-only widget"
          >
            <Minimize2 size={13} />
          </button>
          <button
            onClick={handleToggleAlwaysOnTop}
            className="p-1 rounded cursor-pointer"
            style={{ color: isAlwaysOnTop ? 'var(--purple-bright)' : 'var(--text-muted)' }}
            title={isAlwaysOnTop ? 'Always on top (active)' : 'Pin window'}
          >
            <Pin size={13} className={isAlwaysOnTop ? 'fill-purple-500/30' : ''} />
          </button>
          <button
            onClick={handleMinimize}
            className="p-1 rounded cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
            title="Minimize widget"
          >
            <Minus size={13} />
          </button>
          <button
            onClick={handleClose}
            className="p-1 rounded cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
            title="Hide widget"
          >
            <X size={14} />
          </button>
        </div>
      </div>

      <div className="flex-1 flex flex-col justify-center gap-2 mt-1.5 min-h-0">
        {fiveHourWindow && (
          <LimitRow
            label="5-Hour"
            icon={<Clock3 size={12} />}
            window={fiveHourWindow}
          />
        )}
        {weeklyWindow && (
          <LimitRow
            label="Weekly"
            icon={<CalendarDays size={12} />}
            window={weeklyWindow}
          />
        )}
        {!fiveHourWindow && !weeklyWindow && (
          <div className="flex-1 flex items-center justify-center text-xs" style={{ color: 'var(--text-muted)' }}>
            Waiting for Codex usage data…
          </div>
        )}
      </div>
      <ResizeHandles />
    </div>
  );
}
