import { create } from 'zustand';

export type ThemeMode = 'light' | 'dark' | 'system';

interface ThemeStore {
  theme: ThemeMode;
  setTheme: (theme: ThemeMode) => void;
  /** Computed: whether dark mode is currently active */
  isDark: boolean;
}

function getSystemIsDark() {
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function applyTheme(mode: ThemeMode) {
  const root = document.documentElement;
  const isDark = mode === 'dark' || (mode === 'system' && getSystemIsDark());
  root.classList.toggle('dark', isDark);
}

// Persist to localStorage key
const STORAGE_KEY = 'voxflow-theme';

export const useThemeStore = create<ThemeStore>((set, get) => {
  const saved = (localStorage.getItem(STORAGE_KEY) as ThemeMode | null) ?? 'system';

  // Apply immediately on init
  applyTheme(saved);

  // Listen for system theme changes
  const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
  mediaQuery.addEventListener('change', () => {
    if (get().theme === 'system') {
      applyTheme('system');
      set({ isDark: getSystemIsDark() });
    }
  });

  return {
    theme: saved,
    isDark: saved === 'system' ? getSystemIsDark() : saved === 'dark',
    setTheme: (theme) => {
      localStorage.setItem(STORAGE_KEY, theme);
      applyTheme(theme);
      set({ theme, isDark: theme === 'system' ? getSystemIsDark() : theme === 'dark' });
    },
  };
});
