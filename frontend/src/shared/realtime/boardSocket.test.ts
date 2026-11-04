import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createBoardSocket, boardLiveUrl } from './boardSocket';
import type { BoardEvent } from '../types';
import { FakeWebSocket, WS, latest } from './fakeWebSocket';

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

  it('dispatches every non-ping event type for the matching project', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    const seen: string[] = [];
    sock.onEvent((e) => seen.push(e.type));

    latest().emit({ type: 'item_created', project_id: 'p1', item_id: 'i', status: 'todo' });
    latest().emit({ type: 'item_updated', project_id: 'p1', item_id: 'i', old_status: 'todo', new_status: 'done' });
    latest().emit({ type: 'item_deleted', project_id: 'p1', item_id: 'i' });
    latest().emit({ type: 'board_config_updated', project_id: 'p1' });
    latest().emit({ type: 'sprint_updated', project_id: 'p1', sprint_id: 's' });

    expect(seen).toEqual([
      'item_created',
      'item_updated',
      'item_deleted',
      'board_config_updated',
      'sprint_updated',
    ]);
    sock.close();
  });

  it('an unrecognised event type on the wire is forwarded, not thrown', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    const seen: unknown[] = [];
    sock.onEvent((e) => seen.push(e));

    // A future BoardEvent variant this client build doesn't know about yet.
    expect(() =>
      latest().emit({ type: 'some_future_event', project_id: 'p1' } as unknown as BoardEvent)
    ).not.toThrow();

    expect(seen).toHaveLength(1);
    expect((seen[0] as { type: string }).type).toBe('some_future_event');
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
});
