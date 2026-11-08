import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { loadDensity, loadHiddenColumns, saveDensity, saveHiddenColumns } from './listPrefs';

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

describe('the remembered columns', () => {
  it('start with the type column hidden when nothing is stored', () => {
    expect([...loadHiddenColumns()]).toEqual(['item_type']);
  });

  it('come back as saved', () => {
    saveHiddenColumns(new Set(['status', 'due_date']));
    expect([...loadHiddenColumns()].sort()).toEqual(['due_date', 'status']);
  });

  it('never hide the title, and ignore a column that no longer exists', () => {
    localStorage.setItem('tack_list_cols', JSON.stringify(['title', 'status', 'sprint_points']));
    expect([...loadHiddenColumns()]).toEqual(['status']);
  });

  it('start from the defaults when the stored value is not a list', () => {
    localStorage.setItem('tack_list_cols', '{not json');
    expect([...loadHiddenColumns()]).toEqual(['item_type']);
  });
});

describe('the remembered row height', () => {
  it('is comfortable when nothing is stored or the value is unknown', () => {
    expect(loadDensity()).toBe('comfortable');
    localStorage.setItem('tack_list_density', 'huge');
    expect(loadDensity()).toBe('comfortable');
  });

  it('comes back as saved', () => {
    saveDensity('compact');
    expect(loadDensity()).toBe('compact');
  });
});

describe('when storage refuses', () => {
  it('reads as nothing chosen and saves without throwing', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => { throw new Error('blocked'); },
      setItem: () => { throw new Error('blocked'); },
      removeItem: () => { throw new Error('blocked'); },
    });
    expect([...loadHiddenColumns()]).toEqual(['item_type']);
    expect(loadDensity()).toBe('comfortable');
    expect(() => saveHiddenColumns(new Set(['status']))).not.toThrow();
    expect(() => saveDensity('compact')).not.toThrow();
  });
});
