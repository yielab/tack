import type { ChangeSet, ItemRef, Operation } from '../api/changeSets';
import { teamOf, type AgentProfileSummary } from '../execution/api';
import { shortItemId } from '../shortItemId';
import type { Row } from './PlanTree';

// What a change set's operations are, in the words the panel shows, and how a tree row maps to and
// from a `create`. Pure: the panel keeps the state.

export type CreateOperation = Extract<Operation, { op: 'create' }>;
export type OperationOf<K extends Operation['op']> = Extract<Operation, { op: K }>;

export const isCreate = (op: Operation): op is CreateOperation => op.op === 'create';

export const PRIORITIES = ['critical', 'high', 'medium', 'low', 'none'] as const;
const PRIORITY_LABEL: Record<string, string> = { critical: 'Critical', high: 'High', medium: 'Medium', low: 'Low', none: 'None' };
export const priorityLabel = (priority: string) => PRIORITY_LABEL[priority] ?? priority;

/** How each kind of operation is named in a list. */
export const KIND_LABEL: Record<Operation['op'], string> = {
  create: 'New item',
  who_does_it: 'Who does it',
  priority: 'Priority',
  move: 'Move',
  depend: 'Dependency',
  comment: 'Comment',
  due: 'Due date',
};

/** The place in the change set's operations of each `create`, in order. */
export const createPositions = (ops: readonly Operation[]): number[] => ops.flatMap((op, i) => (isCreate(op) ? [i] : []));

/** A principal label as a person reads it: the credential's name, or who a label without one stands for. */
export function proposerName(set: Pick<ChangeSet, 'principal' | 'principal_name'>): string {
  if (set.principal_name) return set.principal_name;
  if (set.principal.startsWith('operator')) return 'You';
  if (set.principal.startsWith('agent')) return 'An agent';
  return set.principal;
}

/** Where a change set came from. */
export function originText(set: Pick<ChangeSet, 'origin' | 'origin_ref'>): string {
  const ref = set.origin_ref ? ` ${set.origin_ref.length > 12 ? shortItemId(set.origin_ref) : set.origin_ref}` : '';
  if (set.origin === 'run') return `From a run${ref}`;
  if (set.origin === 'thread') return `From a thread${ref}`;
  return `Through the MCP server${ref}`;
}

/** The name an item ref is shown under: a new item by its title, an existing one by the title read for it, else its short id. */
export type NameOf = (ref: ItemRef) => string;

/** One line saying what an operation does; the accessible name of its tick. */
export function summarize(op: Operation, nameOf: NameOf): string {
  switch (op.op) {
    case 'create':
      return `Create ${op.title.trim() || 'an untitled item'}`;
    case 'who_does_it': {
      const who = op.team ? `the team ${op.team}` : op.profile_id ? 'a profile' : 'nobody';
      return `Give ${nameOf(op.item)} to ${who}${op.role ? `, role ${op.role}` : ''}`;
    }
    case 'priority':
      return `Set the priority of ${nameOf(op.item)} to ${priorityLabel(op.priority)}`;
    case 'move':
      return `Move ${nameOf(op.item)} to ${op.status}`;
    case 'depend':
      return `${nameOf(op.blocked)} waits on ${nameOf(op.blocker)}`;
    case 'comment':
      return `Comment on ${nameOf(op.item)}`;
    case 'due':
      return op.due ? `Set the due date of ${nameOf(op.item)} to ${op.due.slice(0, 10)}` : `Clear the due date of ${nameOf(op.item)}`;
  }
}

/** The existing items an operation names: the ones whose titles are worth reading. */
export function existingItems(ops: readonly Operation[]): string[] {
  const found = new Set<string>();
  const note = (ref: ItemRef | null | undefined) => {
    if (typeof ref === 'string') found.add(ref);
  };
  for (const op of ops) {
    if (op.op === 'create') {
      note(op.parent);
      op.depends_on?.forEach(note);
    } else if (op.op === 'depend') {
      note(op.blocked);
      note(op.blocker);
    } else {
      note(op.item);
    }
  }
  return [...found].sort();
}

/** The team a create names, or the team of the profile it names. */
const teamOfCreate = (op: CreateOperation, profiles: readonly AgentProfileSummary[]): string | null =>
  op.team ?? (op.profile_id ? teamOf(profiles.find((p) => p.agent_profile_id === op.profile_id)) : null);

/**
 * The tree's rows for the creates of a change set, in order. `ops` are the operations as they stand
 * with any edit on them; `states` and `ticked` are by position in the whole list. A reference to
 * anything but another create in the set is not a row and is kept as it is when a row is saved.
 */
export function createRows(
  ops: readonly Operation[],
  states: readonly ('waiting' | 'applied' | 'rejected')[],
  ticked: ReadonlySet<number>,
  profiles: readonly AgentProfileSummary[],
): Row[] {
  const positions = createPositions(ops);
  const local = new Map(positions.map((pos, i) => [pos, i]));
  const place = (ref: ItemRef | null | undefined): number | null =>
    typeof ref === 'number' && local.has(ref) ? local.get(ref)! : null;
  return positions.map((pos) => {
    const op = ops[pos] as CreateOperation;
    const state = states[pos];
    return {
      title: op.title,
      description: op.description ?? '',
      acceptance: op.acceptance ?? [],
      depends_on: (op.depends_on ?? []).flatMap((ref) => {
        const at = place(ref);
        return at === null ? [] : [at];
      }),
      type: op.type ?? null,
      role: op.role ?? null,
      team: teamOfCreate(op, profiles),
      parent: place(op.parent),
      selected: state === 'waiting' && ticked.has(pos),
      locked: state === 'applied' ? 'Applied' : state === 'rejected' ? 'Turned down' : undefined,
    };
  });
}

/**
 * A create with a row's change put in. Only what the row changed is written, so a reference the tree
 * does not draw (an existing item) and a profile the person did not touch stay as they were.
 */
export function withRowChange(op: CreateOperation, change: Partial<Row>, positions: readonly number[]): CreateOperation {
  const next: CreateOperation = { ...op };
  if (change.title !== undefined) next.title = change.title;
  if (change.description !== undefined) next.description = change.description === '' ? null : change.description;
  if (change.acceptance !== undefined) next.acceptance = change.acceptance;
  if (change.type !== undefined) next.type = change.type;
  if (change.role !== undefined) next.role = change.role;
  if (change.team !== undefined) {
    // A profile wins over a team, so choosing a team puts the profile aside.
    next.team = change.team;
    next.profile_id = null;
  }
  if (change.depends_on !== undefined) {
    const drawn = new Set(positions);
    const kept = (op.depends_on ?? []).filter((ref) => !(typeof ref === 'number' && drawn.has(ref)));
    next.depends_on = [...kept, ...change.depends_on.map((at) => positions[at])];
  }
  return next;
}
