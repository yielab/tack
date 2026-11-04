import { createSignal, getOwner, onCleanup } from 'solid-js';
import { api } from '../api';
import type { InboxNotification, NotificationKind } from '../api/notifications';
import { createNotificationsSocket, type NotificationMessage } from '../realtime/notificationsSocket';
import { notifyNeedsYou } from '../notify';
import { toast } from '../ui/toast';

// What needs the person, as the server's inbox keeps it: the rows most recently kept and how many
// are open. The list is read when the socket connects (and again if the server says it fell
// behind); each change after that is applied here, so nothing is polled.

/** How many of the newest rows are kept: the open ones first in view, then what was read or closed. */
const LIST_LIMIT = 50;

const [rows, setRows] = createSignal<InboxNotification[]>([]);
const [openCount, setOpenCount] = createSignal(0);
const [drawerOpen, setDrawerOpen] = createSignal(false);

/** How many notifications are open, as the server counts them. */
export const openNotificationCount = openCount;
export const inboxOpen = drawerOpen;
export const openInbox = () => setDrawerOpen(true);
export const closeInbox = () => setDrawerOpen(false);

/** A needs-you stays open until what it asks is done; an info stays open until it is read. */
export const isOpenRow = (n: InboxNotification): boolean =>
  !n.resolved_at && (n.level === 'needs_you' || !n.read_at);

/** What still asks something of the person, newest first. */
export const openRows = (): InboxNotification[] => rows().filter(isOpenRow);
/** What was read or closed, newest first. */
export const recentRows = (): InboxNotification[] => rows().filter((n) => !isOpenRow(n));
export const unreadCount = (): number => rows().filter((n) => !n.read_at).length;

/** The page title with the open count in front when something is open. */
export const titleWithCount = (title: string, open: number): string => (open > 0 ? `(${open}) ${title}` : title);

/** What a screen that shows one run needs to know: that something about it opened or closed, or that changes may have been missed. */
export type InboxChange = { kind: NotificationKind; requestId: string | null } | 'resync';

const listeners = new Set<(change: InboxChange) => void>();

/** Subscribe to the inbox's changes; returns an unsubscribe function. */
export function onInboxChange(cb: (change: InboxChange) => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

const tell = (change: InboxChange) => {
  for (const cb of listeners) {
    try {
      cb(change);
    } catch (err) {
      console.error('[notifications] listener threw', err);
    }
  }
};

let fetching = false;
let again = false;
let watching = false;

/** Read the list afresh. A change that comes while it is being read makes it read once more, so the answer is never older than the change. */
async function refetch(): Promise<void> {
  if (fetching) {
    again = true;
    return;
  }
  fetching = true;
  try {
    do {
      again = false;
      const res = await api.notifications.list('all', LIST_LIMIT);
      if (!watching) return;
      setRows(res.notifications);
      setOpenCount(res.open);
    } while (again);
  } catch {
    /* keep what is on screen: the next connection reads it again */
  } finally {
    fetching = false;
  }
}

const withRow = (id: string, change: Partial<InboxNotification>) =>
  setRows((prev) => prev.map((n) => (n.id === id ? { ...n, ...change } : n)));

/** Apply one message of the live socket. */
export function applyNotificationMessage(message: NotificationMessage): void {
  setOpenCount(message.open);
  switch (message.type) {
    case undefined:
      void refetch();
      tell('resync');
      return;
    case 'opened': {
      const { type: _type, open: _open, ...row } = message;
      const known = rows().some((n) => n.id === row.id);
      setRows((prev) => [row, ...prev.filter((n) => n.id !== row.id)].slice(0, LIST_LIMIT));
      if (fetching) again = true;
      if (!known && row.level === 'needs_you') {
        toast.warning(row.title);
        notifyNeedsYou(row.title);
      }
      tell({ kind: row.kind, requestId: row.request_id ?? null });
      return;
    }
    case 'closed': {
      const row = rows().find((n) => n.id === message.id);
      withRow(message.id, { resolved_at: new Date().toISOString() });
      if (row) tell({ kind: row.kind, requestId: row.request_id ?? null });
      return;
    }
    case 'read':
      withRow(message.id, { read_at: new Date().toISOString() });
      return;
  }
}

/** Mark one notification read. */
export async function readNotification(id: string): Promise<void> {
  withRow(id, { read_at: new Date().toISOString() });
  try {
    await api.notifications.read(id);
  } catch (err) {
    toast.error(err instanceof Error ? err.message : 'Could not mark it read');
    void refetch();
  }
}

/** Mark every unread notification read. */
export async function readAllNotifications(): Promise<void> {
  const now = new Date().toISOString();
  setRows((prev) => prev.map((n) => (n.read_at ? n : { ...n, read_at: now })));
  try {
    await api.notifications.readAll();
  } catch (err) {
    toast.error(err instanceof Error ? err.message : 'Could not mark them read');
    void refetch();
  }
}

let watchers = 0;
let socket: ReturnType<typeof createNotificationsSocket> | undefined;

/**
 * Keep the inbox current for as long as the calling component lives; the first one opens the
 * socket and the last one closes it.
 */
export function useNotifications(): void {
  if (watchers === 0) {
    watching = true;
    socket = createNotificationsSocket(applyNotificationMessage);
  }
  watchers += 1;
  if (getOwner()) {
    onCleanup(() => {
      watchers -= 1;
      if (watchers === 0) {
        watching = false;
        socket?.close();
        socket = undefined;
        setRows([]);
        setOpenCount(0);
        setDrawerOpen(false);
      }
    });
  }
}
