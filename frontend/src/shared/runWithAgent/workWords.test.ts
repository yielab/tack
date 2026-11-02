import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { RUN_STATE_LABEL } from '../ui/RunState';
import { runTally, tallySentence } from './cardRun';

// A task has a Status (the column, a person's) and Work (what its newest run reports). The two are
// told apart by their words: a work word that equals a status name would read as the column.

const WORKFLOW_SOURCE = resolve(dirname(fileURLToPath(import.meta.url)), '../../../../crates/tack-core/src/workflow.rs');

/** Every status name the workflow templates give a column. */
function templateStatusNames(): string[] {
  const source = readFileSync(WORKFLOW_SOURCE, 'utf8');
  const named = [...source.matchAll(/StatusDef\s*\{\s*name:\s*"([^"]+)"/g)].map((m) => m[1]);
  // The construction sub-presets list their phases as ("Name", Category) pairs.
  const phases = [...source.matchAll(/\("([^"]+)",\s*(?:Todo|InProgress|Done)\)/g)].map((m) => m[1]);
  return [...named, ...phases];
}

/** The words the screen uses for work: the state labels, the docket second lines and the tally's phrases. */
function workWords(): string[] {
  const tally = tallySentence(runTally({ needs: 2, review: 2, failed: 2 }))
    .split(', ')
    .map((part) => part.replace(/^\d+ /, ''));
  return [
    ...Object.values(RUN_STATE_LABEL),
    'Needs a person',
    'Needs your review',
    'Cancellation requested',
    ...tally,
    ...tallySentence(runTally({ needs: 1, review: 0, failed: 0 })).split(', ').map((part) => part.replace(/^\d+ /, '')),
  ];
}

describe('work words and status names', () => {
  it('reads the templates statuses, not nothing', () => {
    const names = templateStatusNames();
    expect(names.length).toBeGreaterThan(15);
    expect(names).toEqual(expect.arrayContaining(['Backlog', 'To Do', 'In Progress', 'In Review', 'Review', 'Done', 'Foundation']));
  });

  it('no work word equals a status name', () => {
    const statuses = new Set(templateStatusNames().map((n) => n.trim().toLowerCase()));
    const clashes = workWords().filter((w) => statuses.has(w.trim().toLowerCase()));
    expect(clashes).toEqual([]);
  });

  it('keeps the work labels apart from each other', () => {
    const labels = Object.values(RUN_STATE_LABEL).map((l) => l.toLowerCase());
    expect(new Set(labels).size).toBe(labels.length);
  });
});
