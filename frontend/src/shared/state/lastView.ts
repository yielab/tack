const KEY = 'flexpm_last_lens';
const LENSES = ['board', 'list', 'calendar', 'timeline', 'sprint'] as const;
export type Lens = typeof LENSES[number];

export function getLastLens(): Lens {
  try {
    const v = localStorage.getItem(KEY);
    if (v && LENSES.includes(v as Lens)) return v as Lens;
  } catch { /* ignore */ }
  return 'board';
}

export function setLastLens(lens: Lens): void {
  try { localStorage.setItem(KEY, lens); } catch { /* ignore */ }
}
