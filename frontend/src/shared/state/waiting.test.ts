import { describe, it, expect } from 'vitest';
import type { ExecutionSummary } from '../execution';
import type { InboxNotification } from '../api/notifications';
import { notificationHref, waitingRows, type ItemInfo } from './waiting';

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

  it('a run turned away before it started is not a failure to look at', () => {
    const refused = { ...run('r1', 'a', 'failed', t(1)), never_started: true };
    const rows = waitingRows([refused, run('r2', 'b', 'failed', t(1))], {}, { r1: t(30), r2: t(30) }, Date.parse(t(10)));
    expect(rows.map((r) => r.requestId)).toEqual(['r2']);
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

const notice = (over: Partial<InboxNotification>): InboxNotification => ({
  id: 'n1',
  project_id: 'p1',
  kind: 'decision',
  level: 'needs_you',
  key: 'decision:d1',
  item_id: 'item-1',
  request_id: 'req-1',
  title: 'A run is asking a question',
  cause: 'agent',
  created_at: '2026-11-03T10:00:00Z',
  ...over,
});

describe('notificationHref', () => {
  it('goes to the item, on the run when it has one, and nowhere when it names no item', () => {
    expect(notificationHref(notice({ kind: 'run_failed', request_id: 'r 1' }))).toBe('/projects/p1/items/item-1?tab=execution&run=r%201');
    expect(notificationHref(notice({ kind: 'due_soon', request_id: null }))).toBe('/projects/p1/items/item-1');
    expect(notificationHref(notice({ item_id: null }))).toBeNull();
  });

  it('sends a fall in quality to the Agents\' work part of the Overview and a team memory to the team', () => {
    expect(notificationHref(notice({ kind: 'quality_trend', key: 'trend:p1:profile:Coder', item_id: null }))).toBe('/projects/p1/overview?part=agents');
    expect(
      notificationHref(notice({ kind: 'memory', key: 'memory:11111111-1111-1111-1111-111111111111:saas-dev:mem-1', item_id: null })),
    ).toBe('/agents#team-saas-dev');
  });

  it('sends a question asked in a thread to that thread, not to its hidden item', () => {
    expect(notificationHref(notice({ kind: 'decision', thread_id: 'th 1', item_id: 'anchor-1', request_id: 'r1' }))).toBe('/projects/p1/threads?open=th%201');
    expect(notificationHref(notice({ kind: 'decision', thread_id: null, request_id: 'r1' }))).toBe('/projects/p1/items/item-1?tab=execution&run=r1');
  });

  it('sends a change set to its project\'s Needs you, open on that change set', () => {
    expect(notificationHref(notice({ kind: 'change_set', key: 'change_set:cs-1', item_id: null, request_id: null }))).toBe(
      '/projects/p1/needs-you?open=cs-1',
    );
  });
});
