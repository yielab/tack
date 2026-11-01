import { COLUMNS, DEFAULT_HIDDEN, type ColumnKey } from './listRows';

// The List's remembered choices: which columns show and how tall a row is.
// They live in this browser only; storage that is missing or refuses is read
// as "nothing chosen" and the List still draws.

const COLS_KEY = 'tack_list_cols';
const DENSITY_KEY = 'tack_list_density';

// The Table kept the same two choices under these names before it became the List.
const OLD_COLS_KEY = 'tack_table_cols';
const OLD_DENSITY_KEY = 'tack_table_density';

export type Density = 'comfortable' | 'compact';

/** The stored value under `key`, or, when there is none, the one the Table left under `oldKey`, moved to `key`. */
function readOrMove(key: string, oldKey: string, convert: (old: string) => string = (v) => v): string | null {
  try {
    const current = localStorage.getItem(key);
    if (current !== null) return current;
    const old = localStorage.getItem(oldKey);
    if (old === null) return null;
    const moved = convert(old);
    localStorage.setItem(key, moved);
    localStorage.removeItem(oldKey);
    return moved;
  } catch {
    return null;
  }
}

const KNOWN = new Set<string>(COLUMNS.map((c) => c.key));

/** The Table's Review column is the List's Run and Details columns. */
function fromTableKey(key: string): string[] {
  return key === 'needs_review' ? ['run', 'details'] : [key];
}

const PINNED = new Set<string>(COLUMNS.filter((c) => c.pinned).map((c) => c.key));

/** A stored list of column keys as the keys the List has; the title and unknown columns drop out. */
function listKeys(raw: string, translate: (key: string) => string[]): string[] {
  try {
    return (JSON.parse(raw) as unknown[])
      .filter((k): k is string => typeof k === 'string')
      .flatMap(translate)
      .filter((k) => KNOWN.has(k) && !PINNED.has(k));
  } catch {
    return [];
  }
}

/** The columns the person hid; the pinned ones can never be in it. */
export function loadHiddenColumns(): Set<ColumnKey> {
  const raw = readOrMove(COLS_KEY, OLD_COLS_KEY, (old) => JSON.stringify(listKeys(old, fromTableKey)));
  if (raw !== null) {
    try {
      JSON.parse(raw);
      return new Set(listKeys(raw, (k) => [k]) as ColumnKey[]);
    } catch { /* unreadable: start from the defaults */ }
  }
  return new Set(DEFAULT_HIDDEN);
}

export function saveHiddenColumns(hidden: ReadonlySet<ColumnKey>): void {
  try { localStorage.setItem(COLS_KEY, JSON.stringify([...hidden])); } catch { /* a convenience, not state */ }
}

export function loadDensity(): Density {
  const v = readOrMove(DENSITY_KEY, OLD_DENSITY_KEY);
  return v === 'compact' || v === 'comfortable' ? v : 'comfortable';
}

export function saveDensity(density: Density): void {
  try { localStorage.setItem(DENSITY_KEY, density); } catch { /* a convenience, not state */ }
}
