import type { AcceptanceCriterion, BriefConstraint } from '../../shared/api/briefs';

export type CriterionKind = AcceptanceCriterion['kind'];
export type ConstraintKind = BriefConstraint['kind'];

export const CRITERION_KINDS: { value: CriterionKind; label: string }[] = [
  { value: 'command', label: 'Command' },
  { value: 'test', label: 'Test' },
  { value: 'metric', label: 'Metric' },
  { value: 'file', label: 'File exists' },
  { value: 'absent', label: 'File is absent' },
];

export const CONSTRAINT_KINDS: { value: ConstraintKind; label: string }[] = [
  { value: 'forbidden_path', label: 'Forbidden path' },
  { value: 'allowed_dependency', label: 'Allowed dependency' },
  { value: 'max_changed_files', label: 'Max changed files' },
  { value: 'note', label: 'Note' },
];

/** A criterion as edited: every kind's fields as text, converted on save. */
export interface CriterionRow {
  kind: CriterionKind;
  id: string;
  title: string;
  run: string;
  expect_exit: string;
  cwd: string;
  name: string;
  runner: string;
  op: 'lte' | 'gte' | 'eq';
  threshold: string;
  unit: string;
  path: string;
  text: string;
  /** A manual check a person has ticked on the "Done when" list. */
  checked: boolean;
}

/** A constraint as edited: one text value whose meaning follows its kind. */
export interface ConstraintRow {
  kind: ConstraintKind;
  value: string;
}

export const blankCriterion = (kind: CriterionKind, id: string): CriterionRow => ({
  kind,
  id,
  title: '',
  run: '',
  expect_exit: '0',
  cwd: '',
  name: '',
  runner: '',
  op: 'lte',
  threshold: '',
  unit: '',
  path: '',
  text: '',
  checked: false,
});

export const orNull = (s: string) => (s.trim() === '' ? null : s);

export function toRow(c: AcceptanceCriterion): CriterionRow {
  const row = blankCriterion(c.kind, c.id);
  row.title = c.title;
  switch (c.kind) {
    case 'command':
      Object.assign(row, { run: c.run, expect_exit: String(c.expect_exit), cwd: c.cwd ?? '' });
      break;
    case 'test':
      Object.assign(row, { name: c.name, runner: c.runner ?? '' });
      break;
    case 'metric':
      Object.assign(row, { name: c.name, op: c.op, threshold: String(c.threshold), unit: c.unit ?? '' });
      break;
    case 'file':
    case 'absent':
      row.path = c.path;
      break;
    case 'manual':
      row.text = c.text;
      row.checked = c.checked ?? false;
      break;
  }
  return row;
}

export function fromRow(r: CriterionRow): AcceptanceCriterion {
  const { id, title } = r;
  switch (r.kind) {
    case 'command':
      return { kind: 'command', id, title, run: r.run, expect_exit: Number(r.expect_exit), cwd: orNull(r.cwd) };
    case 'test':
      return { kind: 'test', id, title, name: r.name, runner: orNull(r.runner) };
    case 'metric':
      return { kind: 'metric', id, title, name: r.name, op: r.op, threshold: Number(r.threshold), unit: orNull(r.unit) };
    case 'file':
      return { kind: 'file', id, title, path: r.path };
    case 'absent':
      return { kind: 'absent', id, title, path: r.path };
    case 'manual':
      return { kind: 'manual', id, title, text: r.text, ...(r.checked ? { checked: true } : {}) };
  }
}

export function constraintToRow(c: BriefConstraint): ConstraintRow {
  switch (c.kind) {
    case 'forbidden_path':
      return { kind: c.kind, value: c.glob };
    case 'allowed_dependency':
      return { kind: c.kind, value: c.name };
    case 'max_changed_files':
      return { kind: c.kind, value: String(c.n) };
    case 'note':
      return { kind: c.kind, value: c.text };
  }
}

export function constraintFromRow(r: ConstraintRow): BriefConstraint {
  switch (r.kind) {
    case 'forbidden_path':
      return { kind: r.kind, glob: r.value };
    case 'allowed_dependency':
      return { kind: r.kind, name: r.value };
    case 'max_changed_files':
      return { kind: r.kind, n: Number(r.value) };
    case 'note':
      return { kind: r.kind, text: r.value };
  }
}

export const CONSTRAINT_LABEL: Record<ConstraintKind, string> = {
  forbidden_path: 'Glob, e.g. src/legacy/**',
  allowed_dependency: 'Dependency name',
  max_changed_files: 'Number of files',
  note: 'Note',
};

export const controlClass =
  'w-full min-h-9 rounded-md border px-3.5 py-1.5 text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1';
export const controlStyle = {
  'background-color': 'var(--color-bg-base)',
  color: 'var(--color-text-primary)',
  'border-color': 'var(--color-border-medium)',
  '--tw-ring-color': 'var(--color-focus-ring)',
} as const;

/** Split a server message into per-section errors (`acceptance: ...`) and the rest. */
export function sortErrors(message: string): { acceptance?: string; constraints?: string; other?: string } {
  const out: { acceptance?: string; constraints?: string; other?: string } = {};
  for (const line of message.split('\n').map((l) => l.trim()).filter(Boolean)) {
    const m = /^(acceptance|constraints)\b[^:]*:\s*(.*)$/s.exec(line);
    if (m) out[m[1] as 'acceptance' | 'constraints'] = m[2];
    else out.other = out.other ? `${out.other} ${line}` : line;
  }
  return out;
}

