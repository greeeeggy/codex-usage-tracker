import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen, Event } from '@tauri-apps/api/event';
import { MonitorState, MonitorStateResponse, UsageSnapshot, UsageWindow } from '../types/usage';

interface UsageState {
  snapshot: UsageSnapshot | null;
  monitorState: MonitorState;
  errorMessage: string | null;
  detectedClients: { clientType: string; name: string }[];
  isDarkTheme: boolean;

  // Derived getters
  getFiveHourWindow: () => UsageWindow | undefined;
  getWeeklyWindow: () => UsageWindow | undefined;

  // Actions
  init: () => Promise<void>;
  refresh: () => Promise<void>;
  toggleTheme: () => void;
}

export const useUsageStore = create<UsageState>((set, get) => ({
  snapshot: null,
  monitorState: 'dormant',
  errorMessage: null,
  detectedClients: [],
  isDarkTheme: window.matchMedia('(prefers-color-scheme: dark)').matches,

  getFiveHourWindow: () => {
    const { snapshot } = get();
    return snapshot?.windows.find((w) => w.name === 'fiveHour' || w.durationMinutes === 300);
  },

  getWeeklyWindow: () => {
    const { snapshot } = get();
    return snapshot?.windows.find((w) => w.name === 'weekly' || w.durationMinutes === 10080);
  },

  init: async () => {
    try {
      // Get initial state
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

      // Listen for updates
      await listen('usage-updated', (event: Event<UsageSnapshot>) => {
        console.log('Usage updated:', event.payload);
        set({ snapshot: event.payload });
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
    }
  },

  refresh: async () => {
    try {
      await invoke('refresh_usage');
    } catch (err) {
      console.error('Failed to refresh:', err);
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
}));
