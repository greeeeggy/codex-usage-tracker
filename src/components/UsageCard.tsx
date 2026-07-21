import { UsageWindow } from '../types/usage';
import { Countdown } from './Countdown';
import { getRemainingColorText } from '../utils/cn';
import { Clock, Battery } from 'lucide-react';

interface UsageCardProps {
  title: string;
  window: UsageWindow | undefined;
}

export function UsageCard({ title, window }: UsageCardProps) {
  if (!window) {
    return (
      <div className="bg-white/5 border border-white/10 rounded-2xl p-6 flex flex-col items-center justify-center h-48">
        <span className="text-white/40 font-medium">No data</span>
      </div>
    );
  }

  const remaining = Math.round(window.remainingPercent);
  const colorText = getRemainingColorText(remaining);
  
  // Calculate SVG circle properties
  const radius = 48;
  const circumference = 2 * Math.PI * radius;
  const strokeDashoffset = circumference - (remaining / 100) * circumference;

  let strokeColor = '#22c55e'; // green
  if (remaining < 50 && remaining >= 25) strokeColor = '#eab308'; // yellow
  if (remaining < 25 && remaining >= 10) strokeColor = '#f97316'; // orange
  if (remaining < 10) strokeColor = '#ef4444'; // red

  return (
    <div className="bg-white/5 border border-white/10 rounded-2xl p-6 relative overflow-hidden group hover:bg-white/[0.07] transition-colors">
      <h3 className="text-white/80 font-semibold mb-6 flex items-center gap-2 text-lg">
        <Battery className="w-5 h-5 opacity-70" />
        {title}
      </h3>

      <div className="flex items-center justify-between">
        <div className="relative w-32 h-32 flex items-center justify-center">
          {/* Background circle */}
          <svg className="w-full h-full transform -rotate-90 absolute top-0 left-0">
            <circle
              cx="64"
              cy="64"
              r={radius}
              stroke="currentColor"
              strokeWidth="10"
              fill="transparent"
              className="text-white/10"
            />
            {/* Progress circle */}
            <circle
              cx="64"
              cy="64"
              r={radius}
              stroke={strokeColor}
              strokeWidth="10"
              fill="transparent"
              strokeDasharray={circumference}
              strokeDashoffset={strokeDashoffset}
              strokeLinecap="round"
              className="transition-all duration-1000 ease-out"
            />
          </svg>
          <div className="flex flex-col items-center justify-center relative z-10">
            <span className={`text-3xl font-bold ${colorText}`}>
              {remaining}%
            </span>
          </div>
        </div>

        <div className="flex flex-col items-end text-right">
          <div className="mb-4">
            <span className="text-white/50 text-sm block mb-1">Used</span>
            <span className="text-white font-medium text-lg">
              {Math.round(window.usedPercent)}%
            </span>
          </div>
          <div>
            <span className="text-white/50 text-sm block mb-1 flex items-center gap-1 justify-end">
              <Clock className="w-3 h-3" /> Resets in
            </span>
            <span className="text-white font-medium">
              <Countdown resetsAt={window.resetsAt} />
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
