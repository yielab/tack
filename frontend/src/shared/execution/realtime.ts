// The execution domain's one realtime subscription/invalidation path.
// Mirrors `shared/realtime/boardSocket.ts`'s public shape (a status
// accessor, an `onEvent`-style subscribe returning an unsubscribe function,
// and an idempotent teardown) so it reads as the same kind of thing to a
// consumer — but see the honesty note below on why the transport underneath
// is different.
//
// **Why this is polling, not a WebSocket/SSE push channel.** `BoardEvent`
// (`shared/types/index.ts`, backed by
// `crates/tack-api/src/handlers/websocket.rs`) is the only realtime channel
// that exists in this codebase today, and it is scoped to PM board changes.
// The `execution_requests`/`execution_attempts` tables are a distinct
// vocabulary — `Item` != `ExecutionRequest` != `ExecutionAttempt` — with
// **no push channel of their own anywhere in the backend**. Rather than block on a backend feature
// this module doesn't own, it implements the same subscribe/invalidate/
// dispose *contract* backed by a bounded poll, so a caller gets "disposed
// exactly once" and never has to re-invent invalidation, and a future
// `execution_updated`-style `BoardEvent` variant (or a dedicated execution
// WebSocket) can replace the interior of {@link createExecutionRealtime}
// with zero change to `store.ts#connectRealtime`'s call site.
//
// **A poll that knows when to stop.** A tab left open for days must not call
// the board every few seconds, nor keep calling into a sign-in's redirect
// once the session in front of the board ran out. So a tick is skipped while the page is hidden (one runs as soon as it
// shows again), the wait doubles up to a minute while calls cannot reach the
// board, and the poll stops for good once the session ended: only loading the
// page again signs it back in (`api/client.ts#sessionEnded`).

import { sessionEnded, unreachableCount } from '../api/client';

export type ExecutionInvalidationEvent =
  | { scope: 'list' }
  | { scope: 'request'; requestId: string };

export type ExecutionRealtimeStatus = 'active' | 'stopped' | 'disposed';

/** What the poll reads about the connection before it schedules the next tick. */
export interface ExecutionRealtimeHealth {
  /** The session ended: polling again cannot succeed until the page is loaded again. */
  ended: () => boolean;
  /** Calls in a row that could not reach the board. */
  unreachable: () => number;
}

export interface ExecutionRealtimeOptions {
  /** Poll interval in ms. Default 4000 — frequent enough for an operator
   *  watching an in-flight run, bounded so a busy fleet view doesn't
   *  hammer the list endpoint. */
  intervalMs?: number;
  /** The longest wait between ticks while the board cannot be reached. Default 60000. */
  maxIntervalMs?: number;
  /** Injectable timer functions — tests use `vi.useFakeTimers()` and never
   *  a real wall-clock sleep. Defaults to the global
   *  `setTimeout`/`clearTimeout`. */
  setTimeoutImpl?: (handler: () => void, timeoutMs: number) => ReturnType<typeof setTimeout>;
  clearTimeoutImpl?: (handle: ReturnType<typeof setTimeout>) => void;
  /** Defaults to the API client's own session and reachability. */
  health?: ExecutionRealtimeHealth;
  /** The page whose visibility pauses the poll. Defaults to `document`. */
  page?: Pick<Document, 'visibilityState' | 'addEventListener' | 'removeEventListener'>;
  /**
   * Individual request ids to poll beyond the list itself — e.g. an
   * item-detail view watching one in-flight request closely. Read on every
   * tick (not captured once), so the watch set can grow/shrink over the
   * subscription's lifetime without recreating it.
   */
  watchedRequestIds?: () => readonly string[];
}

export interface ExecutionRealtime {
  status: () => ExecutionRealtimeStatus;
  /** Subscribe to invalidation events; returns an unsubscribe function.
   *  Mirrors `boardSocket.ts#onEvent`'s shape exactly. */
  onInvalidate: (cb: (event: ExecutionInvalidationEvent) => void) => () => void;
  /**
   * Tears down the poll timer and clears all listeners. **Idempotent**: a
   * second call is a no-op rather than a double `clearTimeout` or a
   * double listener-teardown — disposed exactly once, no leak, no
   * double-dispose — and `realtime.test.ts` calls this twice to prove it.
   */
  dispose: () => void;
}

const CLIENT_HEALTH: ExecutionRealtimeHealth = { ended: sessionEnded, unreachable: unreachableCount };

/**
 * Creates a bounded-poll invalidation subscription. Every tick emits one
 * `{scope: 'list'}` event (so `store.ts#loadList` can refresh the roster)
 * plus one `{scope: 'request', requestId}` event per id currently returned
 * by `watchedRequestIds()` (so a focused detail view refreshes just that
 * row without waiting for the list's own cadence).
 */
export function createExecutionRealtime(options: ExecutionRealtimeOptions = {}): ExecutionRealtime {
  const {
    intervalMs = 4000,
    maxIntervalMs = 60_000,
    setTimeoutImpl = setTimeout,
    clearTimeoutImpl = clearTimeout,
    health = CLIENT_HEALTH,
    page = typeof document === 'undefined' ? undefined : document,
    watchedRequestIds = () => [],
  } = options;

  let disposed = false;
  let stopped = false;
  let handle: ReturnType<typeof setTimeout> | null = null;
  const listeners = new Set<(event: ExecutionInvalidationEvent) => void>();

  const emit = (event: ExecutionInvalidationEvent) => {
    for (const cb of listeners) {
      try {
        cb(event);
      } catch (err) {
        console.error('[execution/realtime] listener threw', err);
      }
    }
  };

  const hidden = () => page?.visibilityState === 'hidden';

  const cancel = () => {
    if (handle !== null) clearTimeoutImpl(handle);
    handle = null;
  };

  // The wait doubles with each call in a row that could not reach the board, up to the cap.
  const schedule = () => {
    cancel();
    if (disposed || stopped || hidden()) return;
    const wait = Math.min(intervalMs * 2 ** Math.min(health.unreachable(), 16), maxIntervalMs);
    handle = setTimeoutImpl(tick, wait);
  };

  function tick() {
    handle = null;
    if (disposed) return;
    if (health.ended()) {
      stopped = true;
      return;
    }
    if (hidden()) return; // `onVisibility` runs one as soon as the page shows again
    emit({ scope: 'list' });
    for (const requestId of watchedRequestIds()) {
      emit({ scope: 'request', requestId });
    }
    schedule();
  }

  const onVisibility = () => {
    if (disposed || stopped) return;
    if (hidden()) cancel();
    else tick();
  };
  page?.addEventListener('visibilitychange', onVisibility);

  schedule();

  return {
    status: () => (disposed ? 'disposed' : stopped ? 'stopped' : 'active'),
    onInvalidate(cb) {
      if (disposed) return () => {};
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    dispose() {
      if (disposed) return; // idempotent: no double clearTimeout, no double teardown
      disposed = true;
      cancel();
      page?.removeEventListener('visibilitychange', onVisibility);
      listeners.clear();
    },
  };
}
