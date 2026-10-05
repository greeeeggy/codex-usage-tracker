export interface UsageWindow {
  source: 'primary' | 'secondary' | string;
  name: 'fiveHour' | 'weekly' | string;
  durationMinutes: number | null;
  usedPercent: number;
  remainingPercent: number;
  resetsAt: string | null; // ISO timestamp
}

export interface UsageSnapshot {
  accountKey: string | null;
  capturedAt: string;
  limitId: string | null;
  limitName: string | null;
  planType: string | null;
  rateLimitReachedType: string | null;
  credits: { balance: string; hasCredits: boolean; unlimited: boolean } | null;
  windows: UsageWindow[];
  limits: { limitId: string; limitName: string | null; windows: UsageWindow[] }[];
  latestContextWindow: number | null;
  latestContextLoadPercent: number | null;
  latestLastRequestTokens: TokenBreakdown | null;
}

export type MonitorState =
  | 'dormant'
  | 'connecting'
  | 'monitoring'
  | 'gracePeriod'
  | 'authRequired'
  | 'error';

export interface DetectedClientInfo {
  clientType: string;
  name: string;
}

export interface MonitorStateResponse {
  state: MonitorState;
  errorMessage: string | null;
  detectedClients: DetectedClientInfo[];
}

export interface NotificationPayload {
  title: string;
  body: string;
}

export interface TokenBreakdown {
  inputTokens: number;
  cachedInputTokens: number;
  uncachedInputTokens: number;
  outputTokens: number;
  reasoningTokens: number | null;
  totalTokens: number;
}

export interface TokenTotals {
  accountKey: string | null;
  currentSession: TokenBreakdown;
  fiveHourWindow: TokenBreakdown;
  weeklyWindow: TokenBreakdown;
  today: TokenBreakdown;
  currentMonth: TokenBreakdown;
  allTimeRecorded: TokenBreakdown;
}

export interface AccountUsageSummary {
  lifetimeTokens: number | null;
  peakDailyTokens: number | null;
  longestRunningTurnSec: number | null;
  currentStreakDays: number | null;
  longestStreakDays: number | null;
}

export interface DailyUsageBucket {
  startDate: string;
  tokens: number;
}

export interface AccountUsage {
  accountKey: string | null;
  summary: AccountUsageSummary | null;
  dailyUsageBuckets: DailyUsageBucket[] | null;
  fetchedAt?: number | null;
}

export interface AccountDayPage {
  days: { startDate: string; tokens: number; observedAt: number }[];
  total: number;
}

export interface QuotaSampleRow {
  capturedAt: number; // Unix timestamp
  windowKind: string;
  usedPercent: number;
  remainingPercent: number;
}

export interface UsageDeltas {
  sessionDelta: number;
  todayDelta: number;
  peakHourUsed: number;
  sessionsToday: number;
  longestSessionMinutes: number;
}

export interface AppEvent {
  eventType: string;
  label: string;
  timestamp: string;
  description?: string;
}

export interface DetailedTokenUsage {
  inputTokens: number;
  cachedInputTokens: number;
  cacheWriteInputTokens: number;
  uncachedInputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  totalTokens: number;
}

export interface ChatRequestUsage {
  timestamp: string | null;
  model: string | null;
  reasoningEffort: string | null;
  usage: DetailedTokenUsage;
  cacheRate: number;
  estimatedCostUsd: number | null;
}

export interface ChatSessionSummary {
  accountKey: string;
  id: string;
  title: string;
  cwd: string | null;
  originator: string | null;
  source: string | null;
  model: string | null;
  reasoningEffort: string | null;
  createdAt: string | null;
  updatedAt: number;
  usage: DetailedTokenUsage;
  cacheRate: number;
  estimatedCostUsd: number | null;
  latestRequest: ChatRequestUsage | null;
  turnCount: number;
  requestCount: number;
  isCurrent: boolean;
}

export interface ChatMessage {
  id: string | null;
  role: 'user' | 'assistant' | string;
  text: string;
  timestamp: string | null;
}

export interface ChatTurnDetail {
  id: string;
  startedAt: string | null;
  completedAt: string | null;
  model: string | null;
  reasoningEffort: string | null;
  messages: ChatMessage[];
  usage: DetailedTokenUsage;
  cacheRate: number;
  estimatedCostUsd: number | null;
  requests: ChatRequestUsage[];
}

export interface ChatSessionDetail {
  summary: ChatSessionSummary;
  turns: ChatTurnDetail[];
}

export interface LimitPeriod {
  id: number;
  limitId: string;
  limitName: string | null;
  windowKind: string;
  startedAt: number;
  resetsAt: number;
  firstObservedAt: number;
  lastObservedAt: number;
  usedPercent: number;
  status: 'active' | 'completed';
  tokens: TokenBreakdown;
}

export interface LimitHistoryPage { periods: LimitPeriod[]; total: number }

export interface AccountProfile {
  accountKey: string;
  label: string;
  email: string | null;
  planType: string | null;
  isLegacy: boolean;
}
export interface AccountContext { activeAccount: AccountProfile | null; accounts: AccountProfile[] }

export interface PriceRates { input: number; cachedInput: number | null; cacheWrite: number | null; output: number }
export interface PricingCatalog {
  source: string;
  fetchedAt: number | null;
  models: Record<string, { standard: PriceRates; longContext: PriceRates | null; longContextThreshold: number | null }>;
  lastError: string | null;
}
