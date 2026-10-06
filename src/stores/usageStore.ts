import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen, Event } from '@tauri-apps/api/event';
import {
  AccountContext, AccountProfile, AccountUsage, AppEvent, ChatSessionSummary,
  MonitorState, MonitorStateResponse, QuotaSampleRow, TokenTotals,
  UsageDeltas, UsageSnapshot, UsageWindow, PricingCatalog,
} from '../types/usage';

let isInitialized = false;
let initInFlight: Promise<void> | null = null;
let viewRevision = 0;
export type AppPage = 'overview' | 'usage' | 'history' | 'limits' | 'sessions' | 'insights' | 'settings' | 'switch' | 'guard';
const emptyView = {
  snapshot: null, accountUsage: null, currentChat: null, tokenTotals: null,
  quotaHistory: [], usageDeltas: null, recentEvents: [], lastRefreshError: null,
};

interface UsageState {
  snapshot: UsageSnapshot | null;
  accountUsage: AccountUsage | null;
  currentChat: ChatSessionSummary | null;
  pricing: PricingCatalog | null;
  tokenTotals: TokenTotals | null;
  quotaHistory: QuotaSampleRow[];
  usageDeltas: UsageDeltas | null;
  recentEvents: AppEvent[];
  activeAccount: AccountProfile | null;
  accounts: AccountProfile[];
  selectedAccountKey: string | null;
  viewAccountKey: string;
  monitorState: MonitorState;
  errorMessage: string | null;
  detectedClients: { clientType: string; name: string }[];
  isDarkTheme: boolean;
  isRefreshing: boolean;
  lastRefreshError: string | null;
  activePage: AppPage;
  getFiveHourWindow: () => UsageWindow | undefined;
  getWeeklyWindow: () => UsageWindow | undefined;
  init: () => Promise<void>;
  refresh: () => Promise<void>;
  selectAccount: (key: string | null) => void;
  toggleTheme: () => void;
  setActivePage: (page: AppPage) => void;
}

export const useUsageStore = create<UsageState>((set, get) => {
  const loadView = async () => {
    const revision = ++viewRevision;
    const key = get().viewAccountKey;
    const args = { accountKey: key };
    const [snapshot, accountUsage, tokenTotals, currentChat, quotaHistory, recentEvents, usageDeltas] = await Promise.all([
      invoke<UsageSnapshot | null>('get_usage', args).catch(() => null),
      invoke<AccountUsage | null>('get_account_usage', args).catch(() => null),
      invoke<TokenTotals | null>('get_token_totals', args).catch(() => null),
      invoke<ChatSessionSummary | null>('get_current_chat_summary', args).catch(() => null),
      invoke<QuotaSampleRow[]>('get_quota_history', { ...args, windowKind: 'weekly', sinceHours: 168 }).catch(() => []),
      invoke<AppEvent[]>('get_recent_events', args).catch(() => []),
      invoke<UsageDeltas | null>('get_usage_deltas', args).catch(() => null),
    ]);
    if (revision !== viewRevision || key !== get().viewAccountKey) return;
    set({
      snapshot: snapshot?.accountKey === key ? snapshot : null,
      accountUsage: accountUsage?.accountKey === key ? accountUsage : null,
      tokenTotals: tokenTotals?.accountKey === key ? tokenTotals : null,
      currentChat: currentChat?.accountKey === key ? currentChat : null,
      quotaHistory, recentEvents, usageDeltas,
    });
  };
  const loadCurrentChat = async () => {
    const key = get().viewAccountKey;
    const revision = viewRevision;
    const currentChat = await invoke<ChatSessionSummary | null>('get_current_chat_summary', { accountKey: key }).catch(() => null);
    if (key === get().viewAccountKey && revision === viewRevision) {
      set({ currentChat: currentChat?.accountKey === key ? currentChat : null });
    }
  };
  const switchView = () => {
    const key = get().selectedAccountKey ?? get().activeAccount?.accountKey ?? '__signed_out__';
    if (key !== get().viewAccountKey) {
      ++viewRevision;
      set({ ...emptyView, viewAccountKey: key });
    }
    void loadView();
  };
  const updateAccounts = (context: AccountContext) => {
    const before = get().viewAccountKey;
    const key = get().selectedAccountKey ?? context.activeAccount?.accountKey ?? '__signed_out__';
    set({ activeAccount: context.activeAccount, accounts: context.accounts });
    if (before !== key) switchView();
  };
  const loadMonitor = async () => {
    const response = await invoke<MonitorStateResponse>('get_monitor_state');
    set({ monitorState: response.state, errorMessage: response.errorMessage, detectedClients: response.detectedClients });
  };

  return {
    ...emptyView, pricing: null, activeAccount: null, accounts: [],
    selectedAccountKey: null, viewAccountKey: '__signed_out__',
    monitorState: 'dormant', errorMessage: null, detectedClients: [],
    isDarkTheme: true, isRefreshing: false, activePage: 'overview',
    getFiveHourWindow: () => get().snapshot?.windows.find(w => w.name === 'fiveHour' || w.durationMinutes === 300),
    getWeeklyWindow: () => get().snapshot?.windows.find(w => w.name === 'weekly' || w.durationMinutes === 10080),
    selectAccount: key => { set({ selectedAccountKey: key }); switchView(); },
    init: async () => {
      if (initInFlight) return initInFlight;
      initInFlight = (async () => {
        if (!isInitialized) {
          // Listen before hydration so a sign-in change during startup is observed.
          await listen('accounts-updated', (event: Event<AccountContext>) => updateAccounts(event.payload));
          await listen('pricing-updated', (event: Event<PricingCatalog>) => { set({ pricing: event.payload }); void loadCurrentChat(); });
          await listen('usage-updated', (event: Event<UsageSnapshot>) => {
            if (event.payload.accountKey !== get().viewAccountKey) return;
            set({ snapshot: event.payload });
            void loadView();
            void loadMonitor().catch(console.error);
          });
          await listen('token-totals-updated', (event: Event<TokenTotals>) => {
            if (event.payload.accountKey !== get().viewAccountKey) return;
            set({ tokenTotals: event.payload });
            void loadCurrentChat();
          });
          await listen('account-usage-updated', (event: Event<AccountUsage>) => {
            if (event.payload.accountKey === get().viewAccountKey) set({ accountUsage: event.payload });
          });
          await listen('state-changed', () => { void loadMonitor().catch(console.error); });
          isInitialized = true;
        }
        const [context, pricing] = await Promise.all([
          invoke<AccountContext>('get_accounts'),
          invoke<PricingCatalog>('get_pricing').catch(() => null),
          loadMonitor(),
        ]);
        updateAccounts(context);
        set({ pricing });
        await loadView();
      })();
      try { await initInFlight; }
      catch (error) { console.error('Failed to initialize store:', error); }
      finally { initInFlight = null; }
    },
    refresh: async () => {
      if (get().isRefreshing || get().selectedAccountKey !== null && get().selectedAccountKey !== get().activeAccount?.accountKey) return;
      set({ isRefreshing: true, lastRefreshError: null });
      try {
        await invoke('refresh_usage');
        await new Promise(resolve => setTimeout(resolve, 500));
      } catch (error) { set({ lastRefreshError: String(error) }); }
      finally { set({ isRefreshing: false }); }
    },
    toggleTheme: () => {
      const isDarkTheme = !get().isDarkTheme;
      document.documentElement.classList.toggle('dark', isDarkTheme);
      set({ isDarkTheme });
    },
    setActivePage: activePage => set({ activePage }),
  };
});
