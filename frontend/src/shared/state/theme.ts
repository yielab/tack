// Theme persistence + application. Light/dark/system, applied by
// toggling a class on <html>; the token CSS in index.css supplies the values
// (`.dark` overrides, `prefers-color-scheme` fallback when no class is set).
// Pairs with palette.ts (the accent/surface axis).

import { createSignal } from 'solid-js';

export type Theme = 'light' | 'dark' | 'system';

const KEY = 'tack_theme';

export function getStoredTheme(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    if (v === 'light' || v === 'dark' || v === 'system') return v;
  } catch {
    /* localStorage unavailable */
  }
  return 'system';
}

const [theme, setThemeSignal] = createSignal<Theme>(getStoredTheme());

/** Reactive accessor — drives the sidebar theme toggle. */
export const currentTheme = theme;

/** Whether dark is currently rendered (resolves 'system' against the OS). */
export function isDarkActive(): boolean {
  const t = theme();
  if (t === 'dark') return true;
  if (t === 'light') return false;
  return typeof window !== 'undefined' && window.matchMedia
    ? window.matchMedia('(prefers-color-scheme: dark)').matches
    : false;
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
  setThemeSignal(theme);
}

/** Flip between explicit light and dark (used by the sidebar toggle). */
export function toggleTheme(): void {
  setTheme(isDarkActive() ? 'light' : 'dark');
}

/** Apply the persisted theme on startup. */
export function initTheme(): void {
  applyTheme(getStoredTheme());
}
