import type { Change, FolderStatus, RevisionOrigin, RevisionSummary, Verb } from '../../../../shared/api/folder';

/** The plan's sections, as a person reads them. */
const SECTIONS: Record<string, [one: string, many: string]> = {
  project: ['project', 'project'],
  types: ['type', 'types'],
  fields: ['field', 'fields'],
  profiles: ['profile', 'profiles'],
  roles: ['role', 'roles'],
  sprints: ['sprint', 'sprints'],
  automation: ['automation', 'automation'],
  items: ['item', 'items'],
};

const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

export const sectionTitle = (section: string) => capital(SECTIONS[section]?.[1] ?? section);

const counted = (n: number, section: string) => {
  const [one, many] = SECTIONS[section] ?? [section, section];
  return `${n} ${n === 1 ? one : many}`;
};

/** The chip of a row. */
export const VERB_LABEL: Record<Verb, string> = { create: 'New', update: 'Changed', remove: 'Removed', keep: 'Kept' };

/** The status line's verbs: what the board did since the last revision. */
const DONE: Record<Verb, string> = { create: 'created', update: 'changed', remove: 'removed', keep: 'kept' };

const ORIGIN_LABEL: Record<RevisionOrigin, string> = {
  board: 'an export of the board',
  file: 'an import of the folder',
  revert: 'a revert',
};

/** Where a revision came from, in words; an origin the screen does not know is named as the board sent it. */
export const originLabel = (origin: string): string => ORIGIN_LABEL[origin as RevisionOrigin] ?? origin;

/** A value of a field as one short line. */
export function formatValue(value: unknown): string {
  if (value === undefined || value === null || value === '') return '—';
  if (typeof value === 'string') return value.length > 80 ? `“${value.slice(0, 79)}…”` : `“${value}”`;
  if (Array.isArray(value)) return value.length === 0 ? '—' : value.map(formatValue).join(', ');
  if (typeof value === 'object') {
    const text = JSON.stringify(value);
    return text.length > 80 ? `${text.slice(0, 79)}…` : text;
  }
  return String(value);
}

export function formatWhen(iso: string): string {
  const date = new Date(iso.includes('T') || iso.endsWith('Z') ? iso : `${iso.replace(' ', 'T')}Z`);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

export const revisionLine = (r: RevisionSummary) => `Revision #${r.number}, from ${originLabel(r.origin)}, ${formatWhen(r.created_at)}`;

/**
 * The card's status line: where the folder stands against the board. "4 items created and 1 role
 * changed on the board since revision #7".
 */
export function statusLine(status: FolderStatus): string {
  const last = status.last_revision;
  if (!last) return 'No revision yet. Export once to make the folder the plan of record; until then an import removes nothing.';
  const counts = new Map<string, number>();
  for (const change of status.changes) {
    const key = `${change.section}\u0000${change.verb}`;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  if (counts.size === 0) return `The board matches revision #${last.number}.`;
  const parts = [...counts.entries()].map(([key, n]) => {
    const [section, verb] = key.split('\u0000');
    return `${counted(n, section)} ${DONE[verb as Verb]}`;
  });
  const listed = parts.length === 1 ? parts[0] : `${parts.slice(0, -1).join(', ')} and ${parts[parts.length - 1]}`;
  return `${capital(listed)} on the board since revision #${last.number}.`;
}

/** The plan's changes grouped by section, sections in the order the plan names them. */
export function bySection(changes: Change[]): { section: string; changes: Change[] }[] {
  const groups: { section: string; changes: Change[] }[] = [];
  for (const change of changes) {
    const group = groups.find((g) => g.section === change.section);
    if (group) group.changes.push(change);
    else groups.push({ section: change.section, changes: [change] });
  }
  return groups;
}
