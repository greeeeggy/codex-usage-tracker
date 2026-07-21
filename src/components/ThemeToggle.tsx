import { Moon, Sun } from 'lucide-react';
import { useUsageStore } from '../stores/usageStore';

export function ThemeToggle() {
  const isDarkTheme = useUsageStore((state) => state.isDarkTheme);
  const toggleTheme = useUsageStore((state) => state.toggleTheme);

  return (
    <button
      onClick={toggleTheme}
      className="p-2 rounded-full hover:bg-white/10 text-white/70 hover:text-white transition-colors"
      title="Toggle Theme"
    >
      {isDarkTheme ? <Sun className="w-5 h-5" /> : <Moon className="w-5 h-5" />}
    </button>
  );
}
