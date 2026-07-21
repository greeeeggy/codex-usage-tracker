export interface UsageWindow {
  source: 'primary' | 'secondary' | string;
  name: 'fiveHour' | 'weekly' | string;
  durationMinutes: number | null;
  usedPercent: number;
  remainingPercent: number;
  resetsAt: string | null; // ISO timestamp
}

export interface UsageSnapshot {
  capturedAt: string;
  limitId: string | null;
  limitName: string | null;
  planType: string | null;
  rateLimitReachedType: string | null;
  credits: { balance: string; hasCredits: boolean; unlimited: boolean } | null;
  windows: UsageWindow[];
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
