import { createSignal, type Accessor } from 'solid-js';
import type { BoardEvent } from '../types';

export type SocketStatus = 'connecting' | 'open' | 'reconnecting' | 'closed';

export interface BoardSocket {
  /** Reactive connection status. */
  status: Accessor<SocketStatus>;
  /** Subscribe to board events; returns an unsubscribe function. */
  onEvent: (cb: (event: BoardEvent) => void) => () => void;
  /** Close intentionally (no reconnect). */
  close: () => void;
}

export interface BoardSocketOptions {
  /** Override the WebSocket URL (defaults to same-origin `/api/.../boards/live`). */
  url?: string;
  /** Inject a WebSocket implementation (for tests). */
  WebSocketImpl?: typeof WebSocket;
  /** Initial reconnect delay in ms (default 1000). */
  initialDelay?: number;
  /** Maximum reconnect delay in ms (default 30000). */
  maxDelay?: number;
}

/** Same-origin board-live URL: ws(s)://<host>/api/projects/<id>/boards/live. */
export function boardLiveUrl(projectId: string): string {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${proto}//${location.host}/api/projects/${projectId}/boards/live`;
}

/**
 * Reconnecting WebSocket for live board updates (T-502).
 *
 * - capped exponential backoff on unexpected close;
 * - filters events to the given `projectId` (the server already filters, this
 *   is belt-and-suspenders for shared connections);
 * - consumes the `ping` keepalive without dispatching it;
 * - exposes a `status` signal: connecting | open | reconnecting | closed.
 */
export function createBoardSocket(
  projectId: string,
  options: BoardSocketOptions = {}
): BoardSocket {
  const {
    url = boardLiveUrl(projectId),
    WebSocketImpl = WebSocket,
    initialDelay = 1000,
    maxDelay = 30000,
  } = options;

  const [status, setStatus] = createSignal<SocketStatus>('connecting');
  const listeners = new Set<(event: BoardEvent) => void>();

  let ws: WebSocket | null = null;
  let delay = initialDelay;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let closed = false;

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

  const scheduleReconnect = () => {
    if (closed) return;
    setStatus('reconnecting');
    reconnectTimer = setTimeout(() => {
      delay = Math.min(delay * 2, maxDelay);
      connect();
    }, delay);
  };

  const connect = () => {
    if (closed) return;
    ws = new WebSocketImpl(url);

    ws.onopen = () => {
      delay = initialDelay; // reset backoff on a healthy connection
      setStatus('open');
    };

    ws.onmessage = (ev: MessageEvent) => {
      try {
        dispatch(JSON.parse(ev.data) as BoardEvent);
      } catch (err) {
        console.error('[boardSocket] failed to parse message', err);
      }
    };

    ws.onclose = () => {
      ws = null;
      if (!closed) scheduleReconnect();
    };

    ws.onerror = () => {
      // Let onclose drive reconnection; just close the broken socket.
      ws?.close();
    };
  };

  connect();

  return {
    status,
    onEvent(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    close() {
      closed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = null;
      listeners.clear();
      ws?.close();
      ws = null;
      setStatus('closed');
    },
  };
}
