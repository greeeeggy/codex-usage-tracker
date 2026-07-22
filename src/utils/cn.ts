import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Get a CSS color string for the remaining-percent level */
export function getRemainingColor(remainingPercent: number): string {
  if (remainingPercent >= 50) return 'var(--green)';
  if (remainingPercent >= 25) return 'var(--warning)';
  if (remainingPercent >= 10) return 'var(--warning)';
  return 'var(--danger)';
}

/** Get a Tailwind text-color class for the remaining-percent level */
export function getRemainingColorClass(remainingPercent: number): string {
  if (remainingPercent >= 50) return 'text-green-400';
  if (remainingPercent >= 25) return 'text-yellow-400';
  if (remainingPercent >= 10) return 'text-orange-400';
  return 'text-red-400';
}

export const getRemainingColorText = getRemainingColorClass;

/** Format a number with K/M suffixes */
export function formatNumber(num: number): string {
  if (num >= 1_000_000) return (num / 1_000_000).toFixed(1) + 'M';
  if (num >= 1_000) return (num / 1_000).toFixed(1) + 'K';
  return num.toString();
}

/** Format a token count as an approximate duration string */
export function formatTokensAsDuration(tokens: number): string {
  // Rough heuristic: ~1000 tokens ≈ 1 minute of active usage
  const minutes = Math.round(tokens / 1000);
  if (minutes < 1) return '< 1m';
  if (minutes < 60) return `${minutes}m`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m > 0 ? `${h}h ${m}m` : `${h}h`;
}
