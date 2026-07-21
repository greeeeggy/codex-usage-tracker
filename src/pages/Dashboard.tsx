import { useEffect } from 'react';
import { useUsageStore } from '../stores/usageStore';
import { UsageCard } from '../components/UsageCard';
import { StatusBadge } from '../components/StatusBadge';
import { ThemeToggle } from '../components/ThemeToggle';
import { TokenTotalsCard } from '../components/TokenTotalsCard';
import { TokenBreakdownCard } from '../components/TokenBreakdownCard';
import { HistoryCharts } from '../components/HistoryCharts';
import { RefreshCw, Activity } from 'lucide-react';
import { format } from 'date-fns';

export function Dashboard() {
  const init = useUsageStore((state) => state.init);
  const refresh = useUsageStore((state) => state.refresh);
  const snapshot = useUsageStore((state) => state.snapshot);
  const tokenTotals = useUsageStore((state) => state.tokenTotals);
  const monitorState = useUsageStore((state) => state.monitorState);
  const errorMessage = useUsageStore((state) => state.errorMessage);

  useEffect(() => {
    init();
  }, [init]);

  const fiveHourWindow = snapshot?.windows.find((w) => w.name === 'fiveHour' || w.durationMinutes === 300);
  const weeklyWindow = snapshot?.windows.find((w) => w.name === 'weekly' || w.durationMinutes === 10080);

  return (
    <div className="h-screen overflow-y-auto bg-neutral-950 text-white p-8 flex flex-col">
      <header className="flex items-center justify-between mb-8">
        <div className="flex items-center gap-4">
          <div className="bg-white/10 p-2 rounded-xl">
            <Activity className="w-6 h-6 text-green-400" />
          </div>
          <div>
            <h1 className="text-2xl font-bold tracking-tight">Codex Meter</h1>
            <div className="flex items-center gap-2 mt-1">
              <span className="text-xs px-2 py-0.5 bg-blue-500/20 text-blue-400 border border-blue-500/30 rounded-md uppercase font-medium tracking-wider">
                {snapshot?.planType || 'UNKNOWN PLAN'}
              </span>
              <StatusBadge state={monitorState} errorMessage={errorMessage} />
            </div>
          </div>
        </div>
        <div className="flex items-center gap-4">
          {snapshot && (
            <div className="text-right mr-4">
              <span className="text-xs text-white/50 block">Last Refreshed</span>
              <span className="text-sm font-medium text-white/80">
                {format(new Date(snapshot.capturedAt), 'HH:mm:ss')}
              </span>
            </div>
          )}
          <button
            onClick={refresh}
            className="p-2 rounded-full hover:bg-white/10 text-white/70 hover:text-white transition-colors"
            title="Refresh Data"
            disabled={monitorState === 'error' || monitorState === 'authRequired'}
          >
            <RefreshCw className="w-5 h-5" />
          </button>
          <ThemeToggle />
        </div>
      </header>

      <main className="flex-1">
        {monitorState === 'error' || monitorState === 'authRequired' ? (
          <div className="bg-red-500/10 border border-red-500/20 rounded-2xl p-8 text-center flex flex-col items-center justify-center h-full">
            <div className="bg-red-500/20 p-4 rounded-full mb-4">
              <Activity className="w-8 h-8 text-red-400" />
            </div>
            <h2 className="text-xl font-semibold mb-2">Connection Issue</h2>
            <p className="text-red-200/70 mb-6 max-w-md">{errorMessage}</p>
            <button
              onClick={refresh}
              className="px-6 py-2 bg-white/10 hover:bg-white/20 rounded-lg font-medium transition-colors"
            >
              Retry Connection
            </button>
          </div>
        ) : snapshot ? (
          <div className="space-y-6">
            <div className={`grid gap-6 ${fiveHourWindow && weeklyWindow ? 'grid-cols-2' : 'grid-cols-1'}`}>
              {fiveHourWindow && <UsageCard title="5-Hour Limit" window={fiveHourWindow} />}
              {weeklyWindow && <UsageCard title="Weekly Limit" window={weeklyWindow} />}
              {/* Render any other unexpected windows */}
              {snapshot.windows
                .filter((w) => w.name !== 'fiveHour' && w.durationMinutes !== 300 && w.name !== 'weekly' && w.durationMinutes !== 10080)
                .map((w) => (
                  <UsageCard key={w.name} title={w.name} window={w} />
                ))
              }
            </div>
            
            {tokenTotals && (
              <div className="grid gap-6 grid-cols-1 md:grid-cols-2">
                <TokenTotalsCard totals={tokenTotals} />
                <TokenBreakdownCard breakdown={tokenTotals.currentSession} />
              </div>
            )}

            <HistoryCharts 
              tokenHistory={[]} 
              quotaHistory={[]} 
            />
          </div>
        ) : null}

        {monitorState === 'dormant' && !snapshot && (
          <div className="mt-8 text-center text-white/40">
            Waiting for Codex activity...
          </div>
        )}
      </main>
    </div>
  );
}
