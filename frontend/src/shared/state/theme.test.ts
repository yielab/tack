import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { applyTheme, setTheme, getStoredTheme } from './theme';

// The test env's localStorage is a non-persisting no-op; use a working fake so
// the persistence contract is testable deterministically.
beforeEach(() => {
  const store = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
    clear: () => store.clear(),
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  document.documentElement.classList.remove('light', 'dark');
});

describe('theme', () => {
  it('applyTheme toggles the html class', () => {
    applyTheme('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.classList.contains('light')).toBe(false);

    applyTheme('light');
    expect(document.documentElement.classList.contains('light')).toBe(true);
    expect(document.documentElement.classList.contains('dark')).toBe(false);

    applyTheme('system');
    expect(document.documentElement.classList.contains('light')).toBe(false);
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  it('setTheme persists across reloads and applies the class', () => {
    expect(getStoredTheme()).toBe('system'); // default
    setTheme('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    // a fresh read (as on reload) returns the saved value
    expect(getStoredTheme()).toBe('dark');

    setTheme('light');
    expect(getStoredTheme()).toBe('light');
  });
});
