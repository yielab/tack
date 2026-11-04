import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createNotificationsSocket, notificationsLiveUrl, type NotificationMessage } from './notificationsSocket';
import { FakeWebSocket, WS, latest } from './fakeWebSocket';

describe('createNotificationsSocket', () => {
  beforeEach(() => {
    FakeWebSocket.instances = [];
  });
  afterEach(() => vi.useRealTimers());

  it('connects to /notifications/live', () => {
    expect(notificationsLiveUrl()).toBe(`ws://${location.host}/api/notifications/live`);
  });

  it('passes the count and each change, and nothing that carries no count', () => {
    const seen: NotificationMessage[] = [];
    const sock = createNotificationsSocket((m) => seen.push(m), { WebSocketImpl: WS });
    latest().open();
    latest().emit({ open: 1 });
    latest().emit({ type: 'closed', id: 'n1', key: 'decision:d1', open: 0 });
    latest().emit({ type: 'ping' });
    expect(seen).toEqual([
      { open: 1 },
      { type: 'closed', id: 'n1', key: 'decision:d1', open: 0 },
    ]);
    sock.close();
  });

  it('reconnects, and the server tells the count again', () => {
    vi.useFakeTimers();
    const seen: NotificationMessage[] = [];
    const sock = createNotificationsSocket((m) => seen.push(m), { WebSocketImpl: WS, initialDelay: 10 });
    latest().open();
    latest().serverClose();
    vi.advanceTimersByTime(10);
    latest().open();
    latest().emit({ open: 3 });
    expect(seen).toEqual([{ open: 3 }]);
    sock.close();
  });
});
