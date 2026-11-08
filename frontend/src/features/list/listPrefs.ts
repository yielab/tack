import { COLUMNS, DEFAULT_HIDDEN, type ColumnKey } from './listRows';

// The List's remembered choices: which columns show and how tall a row is.
// They live in this browser only; storage that is missing or refuses is read
// as "nothing chosen" and the List still draws.

const COLS_KEY = 'tack_list_cols';
const DENSITY_KEY = 'tack_list_density';

export type Density = 'comfortable' | 'compact';

/** The stored value under `key`, or `null` when there is none or storage refuses. */
function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

const KNOWN = new Set<string>(COLUMNS.map((c) => c.key));

const PINNED = new Set<string>(COLUMNS.filter((c) => c.pinned).map((c) => c.key));

/** A stored list of column keys as the keys the List has; the title and unknown columns drop out. */
function listKeys(raw: string): string[] {
  return (JSON.parse(raw) as unknown[])
    .filter((k): k is string => typeof k === 'string')
    .filter((k) => KNOWN.has(k) && !PINNED.has(k));
}

/** The columns the person hid; the pinned ones can never be in it. */
export function loadHiddenColumns(): Set<ColumnKey> {
  const raw = read(COLS_KEY);
  if (raw !== null) {
    try {
      return new Set(listKeys(raw) as ColumnKey[]);
    } catch { /* unreadable: start from the defaults */ }
  }
  return new Set(DEFAULT_HIDDEN);
}

/** Whether this browser has a column choice of its own; without one, empty columns stay out of the way. */
export function hasColumnChoice(): boolean {
  try {
    return localStorage.getItem(COLS_KEY) !== null;
  } catch {
    return false;
  }
}

export function saveHiddenColumns(hidden: ReadonlySet<ColumnKey>): void {
  try { localStorage.setItem(COLS_KEY, JSON.stringify([...hidden])); } catch { /* a convenience, not state */ }
}

export function loadDensity(): Density {
  const v = read(DENSITY_KEY);
  return v === 'compact' || v === 'comfortable' ? v : 'comfortable';
}

export function saveDensity(density: Density): void {
  try { localStorage.setItem(DENSITY_KEY, density); } catch { /* a convenience, not state */ }
}
