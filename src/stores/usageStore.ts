import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen, Event } from '@tauri-apps/api/event';
import {
  AccountUsage,
  AppEvent,
  ChatSessionSummary,
  MonitorState,
  MonitorStateResponse,
  QuotaSampleRow,
  TokenTotals,
  UsageDeltas,
  UsageSnapshot,
  UsageWindow,
} from '../types/usage';

let isInitialized = false;
let initInFlight: Promise<void> | null = null;

export type AppPage = 'overview' | 'usage' | 'history' | 'limits' | 'sessions' | 'insights' | 'settings';

interface UsageState {
  snapshot: UsageSnapshot | null;
  accountUsage: AccountUsage | null;
  currentChat: ChatSessionSummary | null;
  tokenTotals: TokenTotals | null;
  quotaHistory: QuotaSampleRow[];
  usageDeltas: UsageDeltas | null;
  recentEvents: AppEvent[];
  monitorState: MonitorState;
  errorMessage: string | null;
  detectedClients: { clientType: string; name: string }[];
  isDarkTheme: boolean;
  isRefreshing: boolean;
  lastRefreshError: string | null;
  activePage: AppPage;

  // Derived getters
  getFiveHourWindow: () => UsageWindow | undefined;
  getWeeklyWindow: () => UsageWindow | undefined;

  // Actions
  init: () => Promise<void>;
  refresh: () => Promise<void>;
  toggleTheme: () => void;
  setActivePage: (page: AppPage) => void;
}

export const useUsageStore = create<UsageState>((set, get) => ({
  snapshot: null,
  accountUsage: null,
  currentChat: null,
  tokenTotals: null,
  quotaHistory: [],
  usageDeltas: null,
  recentEvents: [],
  monitorState: 'dormant',
  errorMessage: null,
  detectedClients: [],
  isDarkTheme: true,
  isRefreshing: false,
  lastRefreshError: null,
  activePage: 'overview',

  getFiveHourWindow: () => {
    const { snapshot } = get();
    return snapshot?.windows.find((w) => w.name === 'fiveHour' || w.durationMinutes === 300);
  },

  getWeeklyWindow: () => {
    const { snapshot } = get();
    return snapshot?.windows.find((w) => w.name === 'weekly' || w.durationMinutes === 10080);
  },

  init: async () => {
    if (initInFlight) return initInFlight;

    initInFlight = (async () => {
      try {
        // Always rehydrate current state. The dashboard webview starts hidden and
        // can mount before the monitor has received its first snapshot.
        const stateResponse = await invoke<MonitorStateResponse>('get_monitor_state');
        set({
          monitorState: stateResponse.state,
          errorMessage: stateResponse.errorMessage,
          detectedClients: stateResponse.detectedClients,
        });

        const snapshot = await invoke<UsageSnapshot | null>('get_usage');
        if (snapshot) {
          set({ snapshot });
        }

        const accountUsage = await invoke<AccountUsage | null>('get_account_usage').catch(() => null);
        if (accountUsage) {
          set({ accountUsage });
        }

        const tokenTotals = await invoke<TokenTotals | null>('get_token_totals').catch(() => null);
        if (tokenTotals) {
          set({ tokenTotals });
        }

        const loadCurrentChat = async () => {
          const currentChat = await invoke<ChatSessionSummary | null>('get_current_chat_summary')
            .catch(() => null);
          if (currentChat) {
            set({ currentChat });
          }
        };
        await loadCurrentChat();

        const loadExtras = async () => {
          try {
            const quotaHistory = await invoke<QuotaSampleRow[]>('get_quota_history', { windowKind: 'weekly', sinceHours: 168 });
            const recentEvents = await invoke<AppEvent[]>('get_recent_events');
            const usageDeltas = await invoke<UsageDeltas>('get_usage_deltas');
            set({ quotaHistory, recentEvents, usageDeltas });
          } catch (e) {
            console.error('Failed to load extra data:', e);
          }
        };
        await loadExtras();

        if (isInitialized) return;

        // Register listeners only once per webview.
        await listen('usage-updated', (event: Event<UsageSnapshot>) => {
          console.log('Usage updated:', event.payload);
          set({ snapshot: event.payload });
          loadExtras(); // Refresh derived metrics on usage update
        });

        await listen('token-totals-updated', (event: Event<TokenTotals>) => {
          console.log('Token totals updated:', event.payload);
          set({ tokenTotals: event.payload });
          void loadCurrentChat();
        });

        await listen('account-usage-updated', (event: Event<AccountUsage>) => {
          console.log('Account usage updated:', event.payload);
          set({ accountUsage: event.payload });
        });

        await listen('state-changed', (event: Event<MonitorState>) => {
          console.log('State changed:', event.payload);
          // Also fetch the full state response to get error messages if any
          invoke<MonitorStateResponse>('get_monitor_state').then((res) => {
            set({
              monitorState: res.state,
              errorMessage: res.errorMessage,
              detectedClients: res.detectedClients,
            });
          });
        });
      } catch (err) {
        console.error('Failed to initialize store:', err);
        throw err;
      }
    })();

    try {
      await initInFlight;
      isInitialized = true;
    } catch {
      // The error was logged above. A later call may retry initialization.
    } finally {
      initInFlight = null;
    }
  },

  refresh: async () => {
    const { isRefreshing } = get();
    if (isRefreshing) return; // Prevent duplicate refreshes

    set({ isRefreshing: true, lastRefreshError: null });
    try {
      await invoke('refresh_usage');
      // Give a brief delay so the UI shows the spinner
      await new Promise((r) => setTimeout(r, 500));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error('Failed to refresh:', msg);
      set({ lastRefreshError: msg });
    } finally {
      set({ isRefreshing: false });
    }
  },

  toggleTheme: () => {
    set((state) => {
      const newDark = !state.isDarkTheme;
      if (newDark) {
        document.documentElement.classList.add('dark');
      } else {
        document.documentElement.classList.remove('dark');
      }
      return { isDarkTheme: newDark };
    });
  },

  setActivePage: (page: AppPage) => {
    set({ activePage: page });
  },
}));
