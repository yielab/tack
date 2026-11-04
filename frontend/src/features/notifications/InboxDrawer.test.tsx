import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import type { InboxNotification } from '../../shared/api/notifications';
import { FakeWebSocket, latest } from '../../shared/realtime/fakeWebSocket';
import { closeInbox, openInbox, useNotifications } from '../../shared/state/notifications';
import { flush } from '../../test/flush';
import InboxDrawer, { notificationHref } from './InboxDrawer';

const row = (id: string, over: Partial<InboxNotification> = {}): InboxNotification => ({
  id,
  project_id: 'p1',
  kind: 'decision',
  level: 'needs_you',
  key: `decision:${id}`,
  item_id: 'item-1',
  request_id: 'req-1',
  title: `Title ${id}`,
  cause: 'agent',
  created_at: '2026-11-03T10:00:00Z',
  read_at: null,
  resolved_at: null,
  ...over,
});

let stopStore: () => void;
let stopView: () => void;
let fetchMock: ReturnType<typeof vi.spyOn>;

function mount(list: { open: number; notifications: InboxNotification[] }) {
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((async (_input: RequestInfo | URL, init?: RequestInit) =>
    init?.method === 'POST'
      ? new Response(null, { status: 204 })
      : new Response(JSON.stringify(list), { status: 200, headers: { 'Content-Type': 'application/json' } })) as typeof fetch);
  createRoot((d) => {
    stopStore = d;
    useNotifications();
  });
  const container = document.createElement('div');
  document.body.appendChild(container);
  stopView = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={InboxDrawer} />
      </MemoryRouter>
    ),
    container,
  );
  latest().open();
  latest().emit({ open: list.open });
  return container;
}

beforeEach(() => {
  FakeWebSocket.instances = [];
  vi.stubGlobal('WebSocket', FakeWebSocket);
});

afterEach(() => {
  closeInbox();
  stopView?.();
  stopStore?.();
  document.body.innerHTML = '';
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const rows = () => [...document.querySelectorAll('[data-testid="inbox-row"]')];

describe('InboxDrawer', () => {
  it('lists what is open first, then what was read or closed, each linking to where it leads', async () => {
    mount({
      open: 2,
      notifications: [
        row('a'),
        row('b', { kind: 'change_set', key: 'change_set:cs-1', item_id: null, request_id: null, title: 'Change set' }),
        row('c', { level: 'info', kind: 'run_finished', read_at: '2026-11-03T11:00:00Z', title: 'Done', key: 'finished:r' }),
      ],
    });
    await flush();
    openInbox();
    await flush();
    expect(rows().map((r) => r.textContent)).toEqual([
      expect.stringContaining('Title a'),
      expect.stringContaining('Change set'),
      expect.stringContaining('Done'),
    ]);
    const headings = [...document.querySelectorAll('h3')].map((h) => h.textContent);
    expect(headings).toEqual(['Open', 'Recent']);
    const hrefs = rows().map((r) => r.querySelector('a')?.getAttribute('href'));
    expect(hrefs[0]).toBe('/projects/p1/items/item-1?tab=execution&run=req-1');
    expect(hrefs[1]).toBe('/projects/p1/change-sets?open=cs-1');
  });

  it('Read marks one row read; Read all is off, with its reason, when nothing is unread', async () => {
    mount({ open: 1, notifications: [row('a', { level: 'info', kind: 'run_finished', key: 'finished:r' })] });
    await flush();
    openInbox();
    await flush();
    const readAll = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Read all')!;
    expect(readAll.disabled).toBe(false);
    document.querySelector<HTMLButtonElement>('button[aria-label="Mark \\"Title a\\" read"]')!.click();
    await flush();
    expect(fetchMock.mock.calls.some(([u, i]) => String(u).endsWith('/notifications/a/read') && i?.method === 'POST')).toBe(true);
    expect(readAll.disabled).toBe(true);
    expect(document.body.textContent).toContain('Nothing unread');
    expect(rows()).toHaveLength(1); // moved to Recent, not gone
  });

  it('says nothing needs the person when nothing is open', async () => {
    mount({ open: 0, notifications: [] });
    await flush();
    openInbox();
    await flush();
    expect(document.querySelector('[data-testid="inbox-open-empty"]')!.textContent).toBe('Nothing needs you.');
  });
});

describe('notificationHref', () => {
  it('goes to the item, on the run when it has one, and nowhere when it names no item', () => {
    expect(notificationHref(row('a', { kind: 'run_failed', request_id: 'r 1' }))).toBe('/projects/p1/items/item-1?tab=execution&run=r%201');
    expect(notificationHref(row('a', { kind: 'due_soon', request_id: null }))).toBe('/projects/p1/items/item-1');
    expect(notificationHref(row('a', { item_id: null }))).toBeNull();
    expect(notificationHref(row('a', { kind: 'memory', key: 'memory:saas-dev:mem-1', item_id: null }))).toBe('/agents#team-saas-dev');
  });
});
