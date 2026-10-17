import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { applyDisplay, getStoredDisplay, setDisplay, resetDisplay, DEFAULT_DISPLAY } from './display';

// A working localStorage fake, as theme.test.ts does: the test env's is a no-op.
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
  applyDisplay(DEFAULT_DISPLAY);
});

const html = () => document.documentElement;

describe('display', () => {
  it('scales the root font size from the browser default and clears it at 100%', () => {
    applyDisplay({ ...DEFAULT_DISPLAY, textSize: 1.25 });
    expect(html().style.fontSize).toBe('125%');
    applyDisplay(DEFAULT_DISPLAY);
    expect(html().style.fontSize).toBe('');
  });

  it('sets and clears the font and motion attributes', () => {
    applyDisplay({ ...DEFAULT_DISPLAY, legibleFont: true, motion: 'reduce' });
    expect(html().getAttribute('data-font')).toBe('legible');
    expect(html().getAttribute('data-motion')).toBe('reduce');
    applyDisplay(DEFAULT_DISPLAY);
    expect(html().hasAttribute('data-font')).toBe(false);
    expect(html().hasAttribute('data-motion')).toBe(false);
  });

  it('persists a change and reads it back as on reload', () => {
    expect(getStoredDisplay()).toEqual(DEFAULT_DISPLAY);
    setDisplay({ textSize: 1.5, legibleFont: true });
    expect(getStoredDisplay()).toEqual({ ...DEFAULT_DISPLAY, textSize: 1.5, legibleFont: true });
    resetDisplay();
    expect(getStoredDisplay()).toEqual(DEFAULT_DISPLAY);
  });

  it('falls back to the default for a stored value it does not offer', () => {
    localStorage.setItem('tack_display', JSON.stringify({ textSize: 9, zoom: 'big', motion: 'fast' }));
    expect(getStoredDisplay()).toEqual(DEFAULT_DISPLAY);
    localStorage.setItem('tack_display', 'not json');
    expect(getStoredDisplay()).toEqual(DEFAULT_DISPLAY);
  });
});
