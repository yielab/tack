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

  it('take the choice the Table stored, once: it is copied under the new name and the old one removed', () => {
    localStorage.setItem('tack_table_cols', JSON.stringify(['assignee', 'needs_review']));
    expect([...loadHiddenColumns()].sort()).toEqual(['assignee', 'details', 'run']);
    expect(localStorage.getItem('tack_table_cols')).toBeNull();
    expect(JSON.parse(localStorage.getItem('tack_list_cols')!).sort()).toEqual(['assignee', 'details', 'run']);

    // A later change is the List's own; nothing is read from the Table again.
    saveHiddenColumns(new Set(['due_date']));
    localStorage.setItem('tack_table_cols', JSON.stringify(['status']));
    expect([...loadHiddenColumns()]).toEqual(['due_date']);
  });

  it('keep what the List stored over what the Table left', () => {
    localStorage.setItem('tack_list_cols', JSON.stringify(['status']));
    localStorage.setItem('tack_table_cols', JSON.stringify(['assignee']));
    expect([...loadHiddenColumns()]).toEqual(['status']);
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

  it('takes the Table’s choice once', () => {
    localStorage.setItem('tack_table_density', 'compact');
    expect(loadDensity()).toBe('compact');
    expect(localStorage.getItem('tack_table_density')).toBeNull();
    expect(localStorage.getItem('tack_list_density')).toBe('compact');
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
