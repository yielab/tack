import { createEffect, createSignal, on, onCleanup, type Accessor } from 'solid-js';
import { api } from '../api';
import type { AutoStartState } from '../api/projects';
import { createBoardSocket } from '../realtime/boardSocket';
import { ITEM_UPDATED_EVENT } from './itemEvents';

/** Why a task that auto-start would start is still waiting: a code for tests and a sentence for people. */
export interface WaitingReason {
  code: string;
  text: string;
}

/** A burst of board events asks once. */
const REFRESH_AFTER_MS = 250;

interface Entry {
  state: Accessor<AutoStartState | undefined>;
  refs: number;
  stop: () => void;
}

const entries = new Map<string, Entry>();

/** One load and one live subscription per project, however many views ask about it. */
function open(projectId: string): Entry {
  const [state, setState] = createSignal<AutoStartState>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;
  const load = async () => {
    try {
      const next = await api.projects.autoStart(projectId);
      if (!stopped && Array.isArray(next?.waiting)) setState(next);
    } catch {
      // The reason is a hint: when it cannot be read, what was shown stays.
    }
  };
  const refresh = () => {
    clearTimeout(timer);
    timer = setTimeout(() => void load(), REFRESH_AFTER_MS);
  };
  void load();
  const socket = createBoardSocket(projectId);
  const off = socket.onEvent(refresh);
  window.addEventListener(ITEM_UPDATED_EVENT, refresh);
  return {
    state,
    refs: 0,
    stop: () => {
      stopped = true;
      clearTimeout(timer);
      off();
      socket.close();
      window.removeEventListener(ITEM_UPDATED_EVENT, refresh);
    },
  };
}

/**
 * Why each task of the project that auto-start would start is still waiting (the column it moves
 * to is full, a blocker is unfinished, the cap on open runs or the token limit is reached).
 * Loaded once per project and read again whenever the board changes.
 */
export function useAutoStart(projectId: () => string | undefined) {
  const [entry, setEntry] = createSignal<Entry>();
  createEffect(
    on(projectId, (id) => {
      if (!id) {
        setEntry(undefined);
        return;
      }
      const shared = entries.get(id) ?? open(id);
      entries.set(id, shared);
      shared.refs += 1;
      setEntry(shared);
      onCleanup(() => {
        shared.refs -= 1;
        if (shared.refs === 0) {
          shared.stop();
          entries.delete(id);
        }
      });
    }),
  );
  /** The reason the task is not started, or null when nothing holds it back. */
  const waiting = (itemId: string): WaitingReason | null =>
    entry()?.state()?.waiting.find((w) => w.item_id === itemId)?.why ?? null;
  return { waiting };
}
