/** A kind of wait in a person's words, and the one action it asks for. */
export interface KindWords {
  label: string;
  action: string;
}

const WORDS: Record<string, KindWords> = {
  decision: { label: 'A question', action: 'Answer' },
  review: { label: 'Ready for review', action: 'Review' },
  run_failed: { label: 'Failed', action: 'See why' },
  change_set: { label: 'Changes to apply', action: 'Review the changes' },
};

/** The words for a kind; a kind with none of its own is a plain wait that opens its task. */
export const wordsOf = (kind: string): KindWords => WORDS[kind] ?? { label: 'Waiting for you', action: 'Open' };

/** How long something has waited: "Waiting 14 min", "Waiting 3 h", "Waiting 2 days". */
export function waitedFor(iso: string, now: number = Date.now()): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return 'Waiting';
  const minutes = Math.max(0, Math.round((now - then) / 60_000));
  if (minutes < 1) return 'Waiting less than a minute';
  if (minutes < 60) return `Waiting ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `Waiting ${hours} h`;
  const days = Math.round(hours / 24);
  return `Waiting ${days} ${days === 1 ? 'day' : 'days'}`;
}
