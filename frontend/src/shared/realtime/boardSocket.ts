import { createSignal, type Accessor } from 'solid-js';
import { apiBaseUrl, tokenStore } from '../api/client';
import type { BoardEvent } from '../types';

const BOARD_PROTOCOL = 'tack.v1';
const AUTH_PROTOCOL_PREFIX = 'tack.auth.';

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
  /** Override the WebSocket URL (defaults to the configured API base). */
  url?: string;
  /** Inject a WebSocket implementation (for tests). */
  WebSocketImpl?: typeof WebSocket;
  /** Initial reconnect delay in ms (default 1000). */
  initialDelay?: number;
  /** Maximum reconnect delay in ms (default 30000). */
  maxDelay?: number;
}

/** Board-live URL derived from the configured HTTP API base, including an
 * intentionally split API origin and any configured base path. */
export function boardLiveUrl(projectId: string): string {
  const api = apiBaseUrl();
  const protocol = api.protocol === 'https:' ? 'wss:' : 'ws:';
  const basePath = api.pathname.replace(/\/$/, '');
  return `${protocol}//${api.host}${basePath}/projects/${encodeURIComponent(projectId)}/boards/live`;
}

function websocketTokenProtocol(token: string): string {
  const binary = Array.from(new TextEncoder().encode(token), (byte) => String.fromCharCode(byte)).join('');
  return `${AUTH_PROTOCOL_PREFIX}${btoa(binary).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '')}`;
}

/**
 * Reconnecting WebSocket for live board updates.
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
    const token = tokenStore.get();
    // Browser WebSockets cannot set Authorization. A base64url subprotocol is
    // request-header data (not a query string) and the server replies only
    // with the fixed `tack.v1` protocol, never echoing this credential.
    const protocols = token ? [BOARD_PROTOCOL, websocketTokenProtocol(token)] : [BOARD_PROTOCOL];
    ws = new WebSocketImpl(url, protocols);

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
