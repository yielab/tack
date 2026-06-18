import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { getLastLens, setLastLens } from './lastView';

function makeLocalStorage() {
  const store = new Map<string, string>();
  return {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => { store.set(k, v); },
    removeItem: (k: string) => { store.delete(k); },
    clear: () => { store.clear(); },
    get length() { return store.size; },
    key: (i: number) => [...store.keys()][i] ?? null,
  };
}

beforeEach(() => {
  vi.stubGlobal('localStorage', makeLocalStorage());
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('getLastLens', () => {
  it('returns "board" when localStorage has no entry', () => {
    expect(getLastLens()).toBe('board');
  });

  it('returns the stored lens after setLastLens', () => {
    setLastLens('list');
    expect(getLastLens()).toBe('list');
  });

  it('returns each valid lens correctly', () => {
    for (const lens of ['board', 'list', 'calendar', 'timeline', 'sprint'] as const) {
      setLastLens(lens);
      expect(getLastLens()).toBe(lens);
    }
  });

  it('returns "board" when the stored value is not a valid lens', () => {
    localStorage.setItem('tack_last_lens', 'invalid_lens');
    expect(getLastLens()).toBe('board');
  });
});

describe('setLastLens', () => {
  it('persists the value so subsequent getLastLens reads it back', () => {
    setLastLens('timeline');
    expect(getLastLens()).toBe('timeline');
    setLastLens('sprint');
    expect(getLastLens()).toBe('sprint');
  });
});
