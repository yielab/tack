import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createRoot } from 'solid-js';
import type { InboxNotification } from '../api/notifications';
import { FakeWebSocket, latest } from '../realtime/fakeWebSocket';
import { useToasts, removeToast } from '../ui/toast';
import { flush } from '../../test/flush';
import {
  inboxOpen,
  isOpenRow,
  onInboxChange,
  openInbox,
  openNotificationCount,
  openRows,
  readAllNotifications,
  readNotification,
  recentRows,
  titleWithCount,
  unreadCount,
  useNotifications,
  type InboxChange,
} from './notifications';

const row = (id: string, over: Partial<InboxNotification> = {}): InboxNotification => ({
  id,
  project_id: 'p1',
  kind: 'decision',
  level: 'needs_you',
  key: `decision:${id}`,
  item_id: 'item-1',
  request_id: 'req-1',
  title: 'A run is asking a question',
  cause: 'agent',
  created_at: '2026-11-03T10:00:00Z',
  read_at: null,
  resolved_at: null,
  ...over,
});

let list: { open: number; notifications: InboxNotification[] };
let fetchMock: ReturnType<typeof vi.spyOn>;
let dispose: () => void;

const listCalls = () => fetchMock.mock.calls.filter(([url, init]) => String(url).includes('/notifications?') && !init?.method);

function start() {
  createRoot((d) => {
    dispose = d;
    useNotifications();
  });
  latest().open();
}

const toastMessages = () => useToasts()().map((t) => t.message);

beforeEach(() => {
  FakeWebSocket.instances = [];
  vi.stubGlobal('WebSocket', FakeWebSocket);
  list = { open: 0, notifications: [] };
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (init?.method === 'POST') return new Response(url.endsWith('/read-all') ? '{"read":1}' : null, { status: url.endsWith('/read-all') ? 200 : 204 });
    return new Response(JSON.stringify(list), { status: 200, headers: { 'Content-Type': 'application/json' } });
  }) as typeof fetch);
});

afterEach(() => {
  dispose?.();
  for (const t of useToasts()()) removeToast(t.id);
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('the inbox store', () => {
  it('reads the list when the socket says the count, and shows the server count', async () => {
    list = { open: 2, notifications: [row('a'), row('b', { level: 'info', kind: 'run_finished' })] };
    start();
    latest().emit({ open: 2 });
    await flush();
    expect(openNotificationCount()).toBe(2);
    expect(openRows().map((n) => n.id)).toEqual(['a', 'b']);
    expect(listCalls()).toHaveLength(1);
    expect(String(listCalls()[0][0])).toContain('state=all');
  });

  it('opens a row from "opened", counts it, and does not read the list again', async () => {
    start();
    latest().emit({ open: 0 });
    await flush();
    latest().emit({ type: 'opened', ...row('a'), open: 1 });
    expect(openNotificationCount()).toBe(1);
    expect(openRows().map((n) => n.id)).toEqual(['a']);
    expect(openRows()[0]).not.toHaveProperty('type');
    expect(listCalls()).toHaveLength(1);
  });

  it('closes a row on "closed": it leaves the open rows for the recent ones and the count falls', async () => {
    list = { open: 1, notifications: [row('a')] };
    start();
    latest().emit({ open: 1 });
    await flush();
    latest().emit({ type: 'closed', id: 'a', key: 'decision:a', open: 0 });
    expect(openNotificationCount()).toBe(0);
    expect(openRows()).toEqual([]);
    expect(recentRows().map((n) => n.id)).toEqual(['a']);
  });

  it('reads a row on "read": an info row leaves the open rows, a needs-you stays open', async () => {
    list = {
      open: 2,
      notifications: [row('a'), row('b', { level: 'info', kind: 'run_finished', key: 'finished:r' })],
    };
    start();
    latest().emit({ open: 2 });
    await flush();
    latest().emit({ type: 'read', id: 'b', open: 1 });
    latest().emit({ type: 'read', id: 'a', open: 1 });
    expect(openNotificationCount()).toBe(1);
    expect(openRows().map((n) => n.id)).toEqual(['a']);
    expect(recentRows().map((n) => n.id)).toEqual(['b']);
    expect(unreadCount()).toBe(0);
  });

  it('a needs-you that arrives is a warning toast; an info one, one already known and the first list are not', async () => {
    list = { open: 1, notifications: [row('old')] };
    start();
    latest().emit({ open: 1 });
    await flush();
    expect(toastMessages()).toEqual([]);

    latest().emit({ type: 'opened', ...row('n1', { title: 'A run failed', kind: 'run_failed' }), open: 2 });
    latest().emit({ type: 'opened', ...row('n1', { title: 'A run failed', kind: 'run_failed' }), open: 2 });
    latest().emit({ type: 'opened', ...row('n2', { level: 'info', title: 'A run finished', kind: 'run_finished' }), open: 3 });
    expect(useToasts()().map((t) => [t.type, t.message])).toEqual([['warning', 'A run failed']]);
  });

  it('reads the list again after the server says it fell behind', async () => {
    start();
    latest().emit({ open: 0 });
    await flush();
    list = { open: 1, notifications: [row('a')] };
    latest().emit({ open: 1 });
    await flush();
    expect(listCalls()).toHaveLength(2);
    expect(openRows().map((n) => n.id)).toEqual(['a']);
  });

  it('tells a screen about a change of a run, and of a closed row by what it was', async () => {
    const seen: InboxChange[] = [];
    const stop = onInboxChange((c) => seen.push(c));
    list = { open: 1, notifications: [row('a')] };
    start();
    latest().emit({ open: 1 });
    await flush();
    latest().emit({ type: 'opened', ...row('b', { request_id: 'req-2' }), open: 2 });
    latest().emit({ type: 'closed', id: 'a', key: 'decision:a', open: 1 });
    latest().emit({ type: 'read', id: 'b', open: 1 });
    stop();
    expect(seen).toEqual([
      'resync',
      { kind: 'decision', requestId: 'req-2' },
      { kind: 'decision', requestId: 'req-1' },
    ]);
  });

  it('Read marks one read at once and asks the server; Read all marks every one', async () => {
    list = { open: 2, notifications: [row('a'), row('b', { level: 'info', kind: 'run_finished', key: 'finished:r' })] };
    start();
    latest().emit({ open: 2 });
    await flush();
    await readNotification('b');
    expect(unreadCount()).toBe(1);
    expect(fetchMock.mock.calls.some(([u, i]) => String(u).endsWith('/notifications/b/read') && i?.method === 'POST')).toBe(true);
    await readAllNotifications();
    expect(unreadCount()).toBe(0);
    expect(fetchMock.mock.calls.some(([u, i]) => String(u).endsWith('/notifications/read-all') && i?.method === 'POST')).toBe(true);
  });

  it('closes the socket and forgets the rows when the last screen leaves', async () => {
    list = { open: 1, notifications: [row('a')] };
    start();
    latest().emit({ open: 1 });
    await flush();
    openInbox();
    dispose();
    expect(latest().closed).toBe(true);
    expect(openRows()).toEqual([]);
    expect(openNotificationCount()).toBe(0);
    expect(inboxOpen()).toBe(false);
  });
});

describe('isOpenRow', () => {
  it('a needs-you is open until resolved, an info one until read', () => {
    expect(isOpenRow(row('a'))).toBe(true);
    expect(isOpenRow(row('a', { read_at: 'x' }))).toBe(true);
    expect(isOpenRow(row('a', { resolved_at: 'x' }))).toBe(false);
    expect(isOpenRow(row('a', { level: 'info' }))).toBe(true);
    expect(isOpenRow(row('a', { level: 'info', read_at: 'x' }))).toBe(false);
  });
});

describe('titleWithCount', () => {
  it('puts the count in front when something is open', () => {
    expect(titleWithCount('Agents · Tack', 3)).toBe('(3) Agents · Tack');
    expect(titleWithCount('Agents · Tack', 0)).toBe('Agents · Tack');
  });
});

