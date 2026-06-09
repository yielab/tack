import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createBoardSocket, boardLiveUrl } from './boardSocket';
import type { BoardEvent } from '../types';

// Minimal WebSocket stand-in we can drive synchronously from tests.
class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  url: string;
  onopen: (() => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;

  constructor(url: string) {
    this.url = url;
    FakeWebSocket.instances.push(this);
  }
  // test drivers
  open() {
    this.onopen?.();
  }
  emit(event: BoardEvent) {
    this.onmessage?.({ data: JSON.stringify(event) });
  }
  serverClose() {
    this.onclose?.();
  }
  close() {
    this.closed = true;
  }
}

const WS = FakeWebSocket as unknown as typeof WebSocket;
const latest = () => FakeWebSocket.instances.at(-1)!;

describe('boardLiveUrl', () => {
  it('builds a same-origin /boards/live url with ws scheme', () => {
    expect(boardLiveUrl('p1')).toBe(
      `ws://${location.host}/api/projects/p1/boards/live`
    );
  });
});

describe('createBoardSocket', () => {
  beforeEach(() => {
    FakeWebSocket.instances = [];
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('starts connecting and flips to open on connect', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    expect(sock.status()).toBe('connecting');
    latest().open();
    expect(sock.status()).toBe('open');
    sock.close();
  });

  it('dispatches matching-project events and filters out other projects', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    const seen: BoardEvent[] = [];
    sock.onEvent((e) => seen.push(e));

    latest().emit({
      type: 'item_updated',
      project_id: 'p1',
      item_id: 'i1',
      old_status: 'todo',
      new_status: 'done',
    });
    latest().emit({
      type: 'item_updated',
      project_id: 'OTHER',
      item_id: 'i9',
      old_status: null,
      new_status: 'done',
    });

    expect(seen).toHaveLength(1);
    expect(seen[0]).toMatchObject({ type: 'item_updated', item_id: 'i1' });
    sock.close();
  });

  it('ignores ping keepalive events', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    const seen: BoardEvent[] = [];
    sock.onEvent((e) => seen.push(e));
    latest().emit({ type: 'ping' });
    expect(seen).toHaveLength(0);
    sock.close();
  });

  it('reconnects with backoff after an unexpected close', () => {
    const sock = createBoardSocket('p1', {
      WebSocketImpl: WS,
      initialDelay: 1000,
    });
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
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    sock.close();
    expect(sock.status()).toBe('closed');

    vi.advanceTimersByTime(60000);
    expect(FakeWebSocket.instances).toHaveLength(1); // no reconnect
  });
});
