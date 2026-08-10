import { createContext, useContext, onMount, onCleanup, type ParentComponent } from 'solid-js';
import { createExecutionStore, createExecutionRealtime, type ExecutionStore } from '../execution';

// The single shared instance of E2's execution store (TODO.md III-E2's own
// note: "E3/E4 should not each call createExecutionStore() independently, or
// they get divergent copies, defeating the 'one consistent state'
// acceptance bar... this card does not add that Provider itself... flagged
// as a small, deliberate omission for E3/E4 (or E6) to wire"). This file is
// that Provider, written by III-E4 the first time a real consumer needed it,
// following the exact shape `shared/state/projectItemsContext.tsx` already
// established for the same kind of "one fetch, shared via context" problem.
//
// Deliberately placed under `shared/state/`, not `shared/execution/**`
// (E2's exclusive ownership per TODO.md III.3) and not
// `frontend/src/features/execution/**` (III-E4's own new UI would naturally
// live there, but `architecture.test.ts` forbids one `features/*` importing
// another `features/*` — Board/item-detail/Sprint all need this, so it has
// to live in `shared/*` regardless of which card "owns" the execution
// feature conceptually; see `shared/runWithAgent/`'s own header comment for
// the fuller reasoning).

const ExecutionStoreContext = createContext<ExecutionStore>();

/**
 * Mount once, above every surface that needs execution data (Board,
 * item-detail, Sprint all live under the app shell — see `app/App.tsx`).
 * Loads the list once on mount and wires E2's bounded-poll realtime
 * invalidation (`createExecutionRealtime`) so every consumer's view of a
 * request updates without a manual refetch or a page navigation — this is
 * the mechanism behind this card's acceptance bar "the request appears in
 * the UI without a page navigation (optimistic/realtime, via E2's store)."
 */
export const ExecutionStoreProvider: ParentComponent = (props) => {
  const store = createExecutionStore();

  onMount(() => {
    void store.loadList();
  });

  const realtime = createExecutionRealtime({
    watchedRequestIds: () => [...store.requests().keys()],
  });
  const unsubscribe = store.connectRealtime(realtime);
  onCleanup(() => {
    unsubscribe();
    realtime.dispose();
  });

  return <ExecutionStoreContext.Provider value={store}>{props.children}</ExecutionStoreContext.Provider>;
};

export function useExecutionStore(): ExecutionStore {
  const ctx = useContext(ExecutionStoreContext);
  if (!ctx) throw new Error('useExecutionStore must be used within ExecutionStoreProvider');
  return ctx;
}
