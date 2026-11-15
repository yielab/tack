import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createExecutionRealtime } from './realtime';
import type { ExecutionInvalidationEvent } from './realtime';

describe('createExecutionRealtime', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('starts active', () => {
    const realtime = createExecutionRealtime();
    expect(realtime.status()).toBe('active');
    realtime.dispose();
  });

  it('emits a list-scope invalidation on every tick', () => {
    const realtime = createExecutionRealtime({ intervalMs: 1000 });
    const seen: ExecutionInvalidationEvent[] = [];
    realtime.onInvalidate((e) => seen.push(e));

    vi.advanceTimersByTime(1000);
    vi.advanceTimersByTime(1000);

    expect(seen).toEqual([{ scope: 'list' }, { scope: 'list' }]);
    realtime.dispose();
  });

  it('also emits one request-scope invalidation per currently watched id', () => {
    let watched: string[] = ['exec_a', 'exec_b'];
    const realtime = createExecutionRealtime({ intervalMs: 1000, watchedRequestIds: () => watched });
    const seen: ExecutionInvalidationEvent[] = [];
    realtime.onInvalidate((e) => seen.push(e));

    vi.advanceTimersByTime(1000);
    expect(seen).toEqual([
      { scope: 'list' },
      { scope: 'request', requestId: 'exec_a' },
      { scope: 'request', requestId: 'exec_b' },
    ]);

    seen.length = 0;
    watched = ['exec_c']; // watch set changes without recreating the subscription
    vi.advanceTimersByTime(1000);
    expect(seen).toEqual([{ scope: 'list' }, { scope: 'request', requestId: 'exec_c' }]);

    realtime.dispose();
  });

  it('dispose() is idempotent: a second call does not clear the timer or listeners twice', () => {
    const clearSpy = vi.fn();
    const realtime = createExecutionRealtime({
      intervalMs: 1000,
      clearTimeoutImpl: clearSpy as unknown as (h: ReturnType<typeof setTimeout>) => void,
    });

    realtime.dispose();
    expect(clearSpy).toHaveBeenCalledTimes(1);
    expect(realtime.status()).toBe('disposed');

    realtime.dispose(); // second call — must be a no-op, not a second clearTimeout
    expect(clearSpy).toHaveBeenCalledTimes(1);
    expect(realtime.status()).toBe('disposed');
  });

  it('dispose() stops further ticks from reaching listeners (no leak)', () => {
    const realtime = createExecutionRealtime({ intervalMs: 1000 });
    const seen: ExecutionInvalidationEvent[] = [];
    realtime.onInvalidate((e) => seen.push(e));

    vi.advanceTimersByTime(1000);
    expect(seen).toHaveLength(1);

    realtime.dispose();
    vi.advanceTimersByTime(5000);
    expect(seen).toHaveLength(1); // no further events after dispose
  });

  it('onEvent-style unsubscribe stops just that listener, not the whole subscription', () => {
    const realtime = createExecutionRealtime({ intervalMs: 1000 });
    const seenA: ExecutionInvalidationEvent[] = [];
    const seenB: ExecutionInvalidationEvent[] = [];
    const unsubA = realtime.onInvalidate((e) => seenA.push(e));
    realtime.onInvalidate((e) => seenB.push(e));

    vi.advanceTimersByTime(1000);
    unsubA();
    vi.advanceTimersByTime(1000);

    expect(seenA).toHaveLength(1);
    expect(seenB).toHaveLength(2);
    realtime.dispose();
  });

  it('subscribing after dispose is a safe no-op unsubscribe', () => {
    const realtime = createExecutionRealtime({ intervalMs: 1000 });
    realtime.dispose();
    const unsub = realtime.onInvalidate(() => {
      throw new Error('must never fire after dispose');
    });
    expect(() => unsub()).not.toThrow();
    vi.advanceTimersByTime(5000);
  });

  it('a listener throwing does not stop other listeners or crash the tick', () => {
    const realtime = createExecutionRealtime({ intervalMs: 1000 });
    const seen: ExecutionInvalidationEvent[] = [];
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    realtime.onInvalidate(() => {
      throw new Error('boom');
    });
    realtime.onInvalidate((e) => seen.push(e));

    expect(() => vi.advanceTimersByTime(1000)).not.toThrow();
    expect(seen).toHaveLength(1);
    errorSpy.mockRestore();
    realtime.dispose();
  });

  describe('a poll that knows when to stop', () => {
    let visibility: DocumentVisibilityState;
    const page = {
      get visibilityState() {
        return visibility;
      },
      addEventListener: document.addEventListener.bind(document),
      removeEventListener: document.removeEventListener.bind(document),
    } as unknown as Document;
    const show = (state: DocumentVisibilityState) => {
      visibility = state;
      document.dispatchEvent(new Event('visibilitychange'));
    };

    beforeEach(() => {
      visibility = 'visible';
    });

    it('skips ticks while the page is hidden and runs one as soon as it shows again', () => {
      const realtime = createExecutionRealtime({ intervalMs: 1000, page });
      const seen: ExecutionInvalidationEvent[] = [];
      realtime.onInvalidate((e) => seen.push(e));

      show('hidden');
      vi.advanceTimersByTime(10_000);
      expect(seen).toHaveLength(0);

      show('visible');
      expect(seen).toHaveLength(1); // at once, not a tick later
      vi.advanceTimersByTime(1000);
      expect(seen).toHaveLength(2);
      realtime.dispose();
    });

    it('waits twice as long for each call in a row that could not reach the board, up to the cap', () => {
      let unreachable = 0;
      const realtime = createExecutionRealtime({
        intervalMs: 1000,
        maxIntervalMs: 5000,
        page,
        health: { ended: () => false, unreachable: () => unreachable },
      });
      const seen: ExecutionInvalidationEvent[] = [];
      realtime.onInvalidate((e) => seen.push(e));

      vi.advanceTimersByTime(1000);
      expect(seen).toHaveLength(1);

      unreachable = 2; // the next wait is 4 s
      vi.advanceTimersByTime(1000); // the wait already scheduled
      expect(seen).toHaveLength(2);
      vi.advanceTimersByTime(3999);
      expect(seen).toHaveLength(2);
      vi.advanceTimersByTime(1);
      expect(seen).toHaveLength(3);

      unreachable = 10; // capped at 5 s
      vi.advanceTimersByTime(4000);
      vi.advanceTimersByTime(4999);
      expect(seen).toHaveLength(4);
      vi.advanceTimersByTime(1);
      expect(seen).toHaveLength(5);

      unreachable = 0; // reached again: back to the interval
      vi.advanceTimersByTime(5000);
      vi.advanceTimersByTime(1000);
      expect(seen).toHaveLength(7);
      realtime.dispose();
    });

    it('stops for good once the session ended, even when the page shows again', () => {
      let ended = false;
      const realtime = createExecutionRealtime({
        intervalMs: 1000,
        page,
        health: { ended: () => ended, unreachable: () => 0 },
      });
      const seen: ExecutionInvalidationEvent[] = [];
      realtime.onInvalidate((e) => seen.push(e));

      vi.advanceTimersByTime(1000);
      ended = true;
      vi.advanceTimersByTime(60_000);
      show('hidden');
      show('visible');
      expect(seen).toHaveLength(1);
      expect(realtime.status()).toBe('stopped');
      realtime.dispose();
    });
  });
});
