import type { Accessor } from 'solid-js';
import type { BoardEvent } from '../types';
import {
  createReconnectingSocket,
  liveUrl,
  type ReconnectingSocketOptions,
  type SocketStatus,
} from './socket';

export interface BoardSocket {
  /** Reactive connection status. */
  status: Accessor<SocketStatus>;
  /** Subscribe to board events; returns an unsubscribe function. */
  onEvent: (cb: (event: BoardEvent) => void) => () => void;
  /** Close intentionally (no reconnect). */
  close: () => void;
}

export interface BoardSocketOptions extends ReconnectingSocketOptions {
  /** Override the WebSocket URL (defaults to the configured API base). */
  url?: string;
}

export function boardLiveUrl(projectId: string): string {
  return liveUrl(`/projects/${encodeURIComponent(projectId)}/boards/live`);
}

/**
 * Live board updates for one project.
 *
 * - filters events to the given `projectId` (the server already filters, this
 *   is belt-and-suspenders for shared connections);
 * - consumes the `ping` keepalive without dispatching it.
 */
export function createBoardSocket(
  projectId: string,
  options: BoardSocketOptions = {}
): BoardSocket {
  const { url = boardLiveUrl(projectId), ...socketOptions } = options;
  const listeners = new Set<(event: BoardEvent) => void>();

  const dispatch = (event: BoardEvent) => {
    // `ping` is a keepalive — it keeps the socket alive but is not a board change.
    if (event.type === 'ping') return;
    if ('project_id' in event && event.project_id !== projectId) return;
    for (const cb of listeners) {
      try {
        cb(event);
      } catch (err) {
        console.error('[boardSocket] listener threw', err);
      }
    }
  };

  const socket = createReconnectingSocket(url, (message) => dispatch(message as BoardEvent), socketOptions);

  return {
    status: socket.status,
    onEvent(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    close() {
      listeners.clear();
      socket.close();
    },
  };
}
