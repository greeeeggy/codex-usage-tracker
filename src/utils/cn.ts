import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// Common color function
export function getRemainingColor(remainingPercent: number): string {
  if (remainingPercent >= 50) return 'bg-green-500';
  if (remainingPercent >= 25) return 'bg-yellow-500';
  if (remainingPercent >= 10) return 'bg-orange-500';
  return 'bg-red-500';
}

export function getRemainingColorText(remainingPercent: number): string {
  if (remainingPercent >= 50) return 'text-green-500';
  if (remainingPercent >= 25) return 'text-yellow-500';
  if (remainingPercent >= 10) return 'text-orange-500';
  return 'text-red-500';
}
