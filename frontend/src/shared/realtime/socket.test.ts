import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createReconnectingSocket, liveUrl } from './socket';
import { FakeWebSocket, WS, latest } from './fakeWebSocket';

describe('liveUrl', () => {
  it('puts the ws scheme and the API base in front of the path', () => {
    expect(liveUrl('/notifications/live')).toBe(`ws://${location.host}/api/notifications/live`);
  });
});

describe('createReconnectingSocket', () => {
  beforeEach(() => {
    FakeWebSocket.instances = [];
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('starts connecting and flips to open on connect', () => {
    const sock = createReconnectingSocket('ws://x/live', () => {}, { WebSocketImpl: WS });
    expect(sock.status()).toBe('connecting');
    latest().open();
    expect(sock.status()).toBe('open');
    sock.close();
  });

  it('offers the fixed protocol and an auth subprotocol without using a query token', () => {
    const sessionKey = `tack_api_token:${location.origin}`;
    sessionStorage.setItem(sessionKey, 'operator-token');
    const sock = createReconnectingSocket('ws://x/live', () => {}, { WebSocketImpl: WS });

    expect(latest().url).not.toContain('?');
    expect(latest().protocols).toEqual(['tack.v1', 'tack.auth.b3BlcmF0b3ItdG9rZW4']);
    sock.close();
    sessionStorage.removeItem(sessionKey);
  });

  it('offers only the fixed protocol when there is no token', () => {
    const sock = createReconnectingSocket('ws://x/live', () => {}, { WebSocketImpl: WS });
    expect(latest().protocols).toEqual(['tack.v1']);
    sock.close();
  });

  it('hands each message to the caller, parsed', () => {
    const seen: unknown[] = [];
    const sock = createReconnectingSocket('ws://x/live', (m) => seen.push(m), { WebSocketImpl: WS });
    latest().open();
    latest().emit({ open: 2 });
    expect(seen).toEqual([{ open: 2 }]);
    sock.close();
  });

  it('a message that is not JSON, or a handler that throws, does not break the socket', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    let calls = 0;
    const sock = createReconnectingSocket(
      'ws://x/live',
      () => {
        calls += 1;
        throw new Error('boom');
      },
      { WebSocketImpl: WS },
    );
    latest().open();
    expect(() => latest().emitRaw('not json')).not.toThrow();
    expect(() => latest().emit({ open: 1 })).not.toThrow();
    expect(calls).toBe(1);
    expect(sock.status()).toBe('open');
    error.mockRestore();
    sock.close();
  });

  it('reconnects with backoff after an unexpected close', () => {
    const sock = createReconnectingSocket('ws://x/live', () => {}, { WebSocketImpl: WS, initialDelay: 1000 });
    latest().open();
    expect(FakeWebSocket.instances).toHaveLength(1);

    latest().serverClose();
    expect(sock.status()).toBe('reconnecting');

    vi.advanceTimersByTime(1000);
    expect(FakeWebSocket.instances).toHaveLength(2); // a fresh socket was created
    latest().open();
    expect(sock.status()).toBe('open');
    sock.close();
  });

  it('does not reconnect after an intentional close', () => {
    const sock = createReconnectingSocket('ws://x/live', () => {}, { WebSocketImpl: WS });
    latest().open();
    sock.close();
    expect(sock.status()).toBe('closed');
    expect(latest().closed).toBe(true);

    vi.advanceTimersByTime(60000);
    expect(FakeWebSocket.instances).toHaveLength(1); // no reconnect
  });
});
