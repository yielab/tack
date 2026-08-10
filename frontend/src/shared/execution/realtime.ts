// The execution domain's "one realtime subscription/invalidation path"
// (TODO.md III-E2 task). Mirrors `shared/realtime/boardSocket.ts`'s public
// shape (a status accessor, an `onEvent`-style subscribe returning an
// unsubscribe function, and an idempotent teardown) so it reads as the same
// kind of thing to a consumer — but see the honesty note below on why the
// transport underneath is different.
//
// **Why this is polling, not a WebSocket/SSE push channel.** `BoardEvent`
// (`shared/types/index.ts`, backed by
// `crates/tack-api/src/handlers/websocket.rs`) is the only realtime channel
// that exists in this codebase today, and it is scoped to PM board changes
// plus Part II's docket-orchestration mirror
// (`agent_run_updated`/`approval_pending` — see `boardSocket.test.ts`, and
// `shared/agentActivity/api.ts`'s header comment on that same domain). Part
// III's `execution_requests`/`execution_attempts` tables are a distinct
// vocabulary (III.0: "`Item` != `ExecutionRequest` != `ExecutionAttempt`")
// with **no push channel of their own anywhere in the backend** — reusing
// `agent_run_updated` for this domain would be exactly the kind of
// vocabulary collision III.0 warns against, since that event is specifically
// about `orch_runs` rows, not `execution_attempts` rows. Rather than block
// this card on a backend feature no other Wave-4 card owns, this module
// implements the same subscribe/invalidate/dispose *contract* backed by a
// bounded poll, so every acceptance bullet ("disposed exactly once",
// consumers never re-invent invalidation) is met today, and a future
// `execution_updated`-style `BoardEvent` variant (or a dedicated execution
// WebSocket) can replace the interior of {@link createExecutionRealtime}
// with zero change to `store.ts#connectRealtime`'s call site. Flagged in
// `docs/agent-handoffs/part-iii/III-E2.md`.

export type ExecutionInvalidationEvent =
  | { scope: 'list' }
  | { scope: 'request'; requestId: string };

export type ExecutionRealtimeStatus = 'active' | 'disposed';

export interface ExecutionRealtimeOptions {
  /** Poll interval in ms. Default 4000 — frequent enough for an operator
   *  watching an in-flight run, bounded so a busy fleet view doesn't
   *  hammer the list endpoint. */
  intervalMs?: number;
  /** Injectable timer functions — tests use `vi.useFakeTimers()` and never
   *  a real wall-clock sleep (TODO.md III.2 rule 9). Defaults to the
   *  global `setInterval`/`clearInterval`. */
  setIntervalImpl?: (handler: () => void, timeoutMs: number) => ReturnType<typeof setInterval>;
  clearIntervalImpl?: (handle: ReturnType<typeof setInterval>) => void;
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
   * second call is a no-op rather than a double `clearInterval` or a
   * double listener-teardown — this is what TODO.md's "disposed exactly
   * once (no leak, no double-dispose)" acceptance bar means in practice,
   * and `realtime.test.ts` calls this twice to prove it.
   */
  dispose: () => void;
}

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
    setIntervalImpl = setInterval,
    clearIntervalImpl = clearInterval,
    watchedRequestIds = () => [],
  } = options;

  let disposed = false;
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

  const tick = () => {
    if (disposed) return;
    emit({ scope: 'list' });
    for (const requestId of watchedRequestIds()) {
      emit({ scope: 'request', requestId });
    }
  };

  const handle = setIntervalImpl(tick, intervalMs);

  return {
    status: () => (disposed ? 'disposed' : 'active'),
    onInvalidate(cb) {
      if (disposed) return () => {};
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    dispose() {
      if (disposed) return; // idempotent: no double clearInterval, no double teardown
      disposed = true;
      clearIntervalImpl(handle);
      listeners.clear();
    },
  };
}
