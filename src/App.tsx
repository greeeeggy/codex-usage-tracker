import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Dashboard } from './pages/Dashboard';
import { Overlay } from './pages/Overlay';
import { AppShell } from './components/AppShell';

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    setWindowLabel(appWindow.label);
  }, []);

  if (!windowLabel) return null;

  if (windowLabel === 'overlay') {
    return <Overlay />;
  }

  return (
    <AppShell>
      <Dashboard />
    </AppShell>
  );
}

export default App;
