import { describe, it, expect } from 'vitest';
import type { ExecutionSummary } from '../execution';
import { waitingRows, type ItemInfo } from './waiting';

const run = (id: string, item: string, state: string, created: string): ExecutionSummary => ({
  request_id: id, item_id: item, state, cancellation_requested_at: null, created_at: created,
});
const info = (title: string, needsReview = false): ItemInfo => ({
  title, projectId: 'p1', needsReview, forRun: '', readAt: 0,
});

describe('waitingRows', () => {
  const t = (m: number) => new Date(Date.UTC(2026, 10, 1, 12, m)).toISOString();

  it('sorts a newest run per item into questions, reviews and failures', () => {
    const rows = waitingRows(
      [
        run('r1', 'a', 'waiting_decision', t(1)),
        run('r2', 'b', 'succeeded', t(2)),
        run('r3', 'c', 'failed', t(3)),
        run('r4', 'd', 'running', t(4)),
      ],
      { a: info('A'), b: info('B', true), c: info('C') },
      {},
      Date.parse(t(0)),
    );
    expect(rows.map((r) => [r.group, r.itemId, r.title])).toEqual([
      ['failed', 'c', 'C'],
      ['review', 'b', 'B'],
      ['ask', 'a', 'A'],
    ]);
  });

  it('a succeeded run nobody needs to review, and a run with a newer run after it, are not rows', () => {
    const rows = waitingRows(
      [run('r1', 'a', 'succeeded', t(1)), run('r2', 'b', 'failed', t(1)), run('r3', 'b', 'succeeded', t(2))],
      { a: info('A', false), b: info('B', false) },
      {},
      Date.parse(t(0)),
    );
    expect(rows).toEqual([]);
  });

  it('a failure counts when it ended after the last look, whatever time its run started', () => {
    const runs = [run('r1', 'a', 'failed', t(1)), run('r2', 'b', 'lost', t(1))];
    const ended = { r1: t(30) };
    const rows = waitingRows(runs, {}, ended, Date.parse(t(10)));
    expect(rows.map((r) => r.requestId)).toEqual(['r1']);
    expect(rows[0].at).toBe(t(30));
  });

  it('a row whose item is not read yet has no title', () => {
    const [row] = waitingRows([run('r1', 'a', 'waiting_decision', t(1))], {}, {}, 0);
    expect(row.title).toBeNull();
    expect(row.projectId).toBeNull();
  });
});
