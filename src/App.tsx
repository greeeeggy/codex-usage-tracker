import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Dashboard } from './pages/Dashboard';
import { Overlay } from './pages/Overlay';
import { useUsageStore } from './stores/usageStore';

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null);
  const isDarkTheme = useUsageStore((state) => state.isDarkTheme);

  useEffect(() => {
    // Add dark mode class if needed
    if (isDarkTheme) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }

    // Determine which window we are running in
    const appWindow = getCurrentWindow();
    setWindowLabel(appWindow.label);
  }, [isDarkTheme]);

  if (!windowLabel) return null;

  if (windowLabel === 'overlay') {
    return <Overlay />;
  }

  return <Dashboard />;
}

export default App;
