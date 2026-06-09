// Theme persistence + application (T-512). Light/dark/system, applied by
// toggling a class on <html>; the token CSS in index.css supplies the values
// (`.dark` overrides, `prefers-color-scheme` fallback when no class is set).

export type Theme = 'light' | 'dark' | 'system';

const KEY = 'flexpm_theme';

export function getStoredTheme(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    if (v === 'light' || v === 'dark' || v === 'system') return v;
  } catch {
    /* localStorage unavailable */
  }
  return 'system';
}

export function applyTheme(theme: Theme): void {
  const el = document.documentElement;
  el.classList.remove('light', 'dark');
  if (theme === 'dark') el.classList.add('dark');
  else if (theme === 'light') el.classList.add('light');
  // 'system' → no explicit class; prefers-color-scheme decides.
}

export function setTheme(theme: Theme): void {
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    /* ignore */
  }
  applyTheme(theme);
}

/** Apply the persisted theme on startup. */
export function initTheme(): void {
  applyTheme(getStoredTheme());
}
