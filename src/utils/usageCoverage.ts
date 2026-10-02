import { AccountUsage, TokenTotals, UsageSnapshot } from '../types/usage';

export function lifetimeCoverage(account: AccountUsage | null, tokens: TokenTotals | null, snapshot: UsageSnapshot | null) {
  const reported = account?.summary?.lifetimeTokens ?? null;
  const local = tokens?.allTimeRecorded.totalTokens ?? null;
  const quotaUsed = snapshot?.windows.some(w => w.usedPercent > 0) ||
    snapshot?.limits?.some(bucket => bucket.windows.some(w => w.usedPercent > 0));
  const usableReport = reported !== null && reported >= (local ?? 0) && !(reported === 0 && quotaUsed);
  return { reported, usableReport, incomplete: reported !== null && !usableReport, value: usableReport ? reported : local };
}
