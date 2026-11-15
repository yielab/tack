import { createSignal, type Accessor } from 'solid-js';
import { apiBaseUrl, renewSession, sessionEnded, tokenStore } from '../api/client';

const LIVE_PROTOCOL = 'tack.v1';
const AUTH_PROTOCOL_PREFIX = 'tack.auth.';

export type SocketStatus = 'connecting' | 'open' | 'reconnecting' | 'closed';

export interface ReconnectingSocket {
  /** Reactive connection status. */
  status: Accessor<SocketStatus>;
  /** Close intentionally (no reconnect). */
  close: () => void;
}

export interface ReconnectingSocketOptions {
  /** Inject a WebSocket implementation (for tests). */
  WebSocketImpl?: typeof WebSocket;
  /** Initial reconnect delay in ms (default 1000). */
  initialDelay?: number;
  /** Maximum reconnect delay in ms (default 30000). */
  maxDelay?: number;
}

/** A live-socket URL under the configured HTTP API base, including an intentionally split API
 * origin and any configured base path. `path` starts with a slash and is relative to that base. */
export function liveUrl(path: string): string {
  const api = apiBaseUrl();
  const protocol = api.protocol === 'https:' ? 'wss:' : 'ws:';
  const basePath = api.pathname.replace(/\/$/, '');
  return `${protocol}//${api.host}${basePath}${path}`;
}

function websocketTokenProtocol(token: string): string {
  const binary = Array.from(new TextEncoder().encode(token), (byte) => String.fromCharCode(byte)).join('');
  return `${AUTH_PROTOCOL_PREFIX}${btoa(binary).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '')}`;
}

/**
 * A WebSocket that stays connected: capped exponential backoff on an unexpected close, the token
 * offered on every attempt, each message parsed as JSON and handed to `onMessage`, and a `status`
 * signal (connecting | open | reconnecting | closed). What a message means is the caller's.
 */
export function createReconnectingSocket(
  url: string,
  onMessage: (message: unknown) => void,
  options: ReconnectingSocketOptions = {}
): ReconnectingSocket {
  const { WebSocketImpl = WebSocket, initialDelay = 1000, maxDelay = 30000 } = options;

  const [status, setStatus] = createSignal<SocketStatus>('connecting');

  let ws: WebSocket | null = null;
  let delay = initialDelay;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let closed = false;

  // A socket refused before it opened may be one whose session cookie ran out: the session is
  // renewed before the next try, so the live view comes back by itself. A session that ended
  // (a sign-in in front of the board) stops the socket: only loading the page again signs it in.
  const scheduleReconnect = (neverOpened: boolean) => {
    if (closed) return;
    if (sessionEnded()) {
      setStatus('closed');
      return;
    }
    setStatus('reconnecting');
    reconnectTimer = setTimeout(() => {
      delay = Math.min(delay * 2, maxDelay);
      if (neverOpened) void renewSession().finally(connect);
      else connect();
    }, delay);
  };

  const connect = () => {
    if (closed) return;
    if (sessionEnded()) {
      setStatus('closed');
      return;
    }
    const token = tokenStore.get();
    // Browser WebSockets cannot set Authorization. A base64url subprotocol is
    // request-header data (not a query string) and the server replies only
    // with the fixed `tack.v1` protocol, never echoing this credential.
    const protocols = token ? [LIVE_PROTOCOL, websocketTokenProtocol(token)] : [LIVE_PROTOCOL];
    ws = new WebSocketImpl(url, protocols);
    let opened = false;

    ws.onopen = () => {
      opened = true;
      delay = initialDelay; // reset backoff on a healthy connection
      setStatus('open');
    };

    ws.onmessage = (ev: MessageEvent) => {
      let message: unknown;
      try {
        message = JSON.parse(ev.data);
      } catch (err) {
        console.error('[socket] failed to parse message', err);
        return;
      }
      try {
        onMessage(message);
      } catch (err) {
        console.error('[socket] message handler threw', err);
      }
    };

    ws.onclose = () => {
      ws = null;
      if (!closed) scheduleReconnect(!opened);
    };

    ws.onerror = () => {
      // Let onclose drive reconnection; just close the broken socket.
      ws?.close();
    };
  };

  connect();

  return {
    status,
    close() {
      closed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = null;
      ws?.close();
      ws = null;
      setStatus('closed');
    },
  };
}
