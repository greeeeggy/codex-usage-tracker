import { ReactNode } from 'react';
import { WindowTitleBar } from './WindowTitleBar';
import { Sidebar } from './Sidebar';

interface AppShellProps {
  children: ReactNode;
}

export function AppShell({ children }: AppShellProps) {
  return (
    <div className="flex flex-col w-screen h-screen overflow-hidden" style={{ background: 'var(--bg-app)' }}>
      {/* Title bar spans full width */}
      <WindowTitleBar />

      {/* Body: sidebar + content */}
      <div className="flex flex-1 overflow-hidden">
        <Sidebar />
        <main
          className="flex-1 overflow-y-auto overflow-x-hidden"
          style={{ background: 'var(--bg-app)' }}
        >
          {children}
        </main>
      </div>
    </div>
  );
}
