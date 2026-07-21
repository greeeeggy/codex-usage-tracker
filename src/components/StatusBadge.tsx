import { MonitorState } from '../types/usage';
import { cn } from '../utils/cn';

interface StatusBadgeProps {
  state: MonitorState;
  errorMessage: string | null;
}

export function StatusBadge({ state, errorMessage }: StatusBadgeProps) {
  let colorClass = 'bg-gray-500';
  let label = 'Dormant';
  let pulse = false;

  switch (state) {
    case 'monitoring':
      colorClass = 'bg-green-500';
      label = 'Monitoring';
      pulse = true;
      break;
    case 'connecting':
      colorClass = 'bg-yellow-500';
      label = 'Connecting...';
      pulse = true;
      break;
    case 'gracePeriod':
      colorClass = 'bg-yellow-500';
      label = 'Grace Period';
      break;
    case 'authRequired':
    case 'error':
      colorClass = 'bg-red-500';
      label = 'Error';
      break;
    case 'dormant':
      colorClass = 'bg-gray-500';
      label = 'Dormant';
      break;
  }

  return (
    <div className="flex items-center gap-2 group relative">
      <div className="relative flex h-3 w-3">
        {pulse && (
          <span className={cn('animate-ping absolute inline-flex h-full w-full rounded-full opacity-75', colorClass)} />
        )}
        <span className={cn('relative inline-flex rounded-full h-3 w-3', colorClass)} />
      </div>
      <span className="text-sm font-medium text-white/70">{label}</span>

      {/* Tooltip */}
      {errorMessage && (
        <div className="absolute hidden group-hover:block top-full mt-2 right-0 w-64 bg-black/90 text-white text-xs p-3 rounded-lg border border-white/10 shadow-xl z-50 pointer-events-none">
          {errorMessage}
        </div>
      )}
    </div>
  );
}
