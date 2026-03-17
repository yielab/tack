import { createSignal, onCleanup } from 'solid-js';

export type BoardEventType =
  | 'ItemCreated'
  | 'ItemUpdated'
  | 'ItemDeleted'
  | 'BoardConfigUpdated'
  | 'SprintUpdated'
  | 'Ping';

export interface BoardEvent {
  event_type: BoardEventType;
  project_id: string;
  item_id?: string;
  sprint_id?: string;
  timestamp: string;
}

export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

export interface WebSocketManager {
  connect: () => void;
  disconnect: () => void;
  status: () => ConnectionStatus;
  onEvent: (callback: (event: BoardEvent) => void) => void;
}

const MAX_RECONNECT_DELAY = 30000; // 30 seconds
const INITIAL_RECONNECT_DELAY = 1000; // 1 second

export function createWebSocketManager(projectId: string, baseUrl?: string): WebSocketManager {
  const [status, setStatus] = createSignal<ConnectionStatus>('disconnected');
  let ws: WebSocket | null = null;
  let reconnectTimeout: number | null = null;
  let reconnectDelay = INITIAL_RECONNECT_DELAY;
  let eventCallbacks: ((event: BoardEvent) => void)[] = [];
  let intentionallyClosed = false;

  const wsUrl = () => {
    const base = baseUrl || window.location.origin;
    const protocol = base.startsWith('https') ? 'wss' : 'ws';
    const host = base.replace(/^https?:\/\//, '');
    return `${protocol}://${host}/api/projects/${projectId}/board/live`;
  };

  const connect = () => {
    if (ws?.readyState === WebSocket.OPEN || ws?.readyState === WebSocket.CONNECTING) {
      return;
    }

    intentionallyClosed = false;
    setStatus('connecting');

    try {
      ws = new WebSocket(wsUrl());

      ws.onopen = () => {
        console.log('[WebSocket] Connected to project:', projectId);
        setStatus('connected');
        reconnectDelay = INITIAL_RECONNECT_DELAY; // Reset reconnect delay on successful connection
      };

      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data) as BoardEvent;

          // Ignore ping events
          if (data.event_type === 'Ping') {
            return;
          }

          console.log('[WebSocket] Received event:', data);

          // Notify all callbacks
          eventCallbacks.forEach((callback) => {
            try {
              callback(data);
            } catch (error) {
              console.error('[WebSocket] Error in event callback:', error);
            }
          });
        } catch (error) {
          console.error('[WebSocket] Failed to parse message:', error);
        }
      };

      ws.onerror = (error) => {
        console.error('[WebSocket] Connection error:', error);
        setStatus('error');
      };

      ws.onclose = (event) => {
        console.log('[WebSocket] Connection closed:', event.code, event.reason);
        setStatus('disconnected');
        ws = null;

        // Auto-reconnect with exponential backoff (unless intentionally closed)
        if (!intentionallyClosed) {
          if (reconnectTimeout) {
            clearTimeout(reconnectTimeout);
          }

          console.log(`[WebSocket] Reconnecting in ${reconnectDelay}ms...`);
          reconnectTimeout = window.setTimeout(() => {
            reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY);
            connect();
          }, reconnectDelay);
        }
      };
    } catch (error) {
      console.error('[WebSocket] Failed to create connection:', error);
      setStatus('error');
    }
  };

  const disconnect = () => {
    intentionallyClosed = true;

    if (reconnectTimeout) {
      clearTimeout(reconnectTimeout);
      reconnectTimeout = null;
    }

    if (ws) {
      ws.close();
      ws = null;
    }

    setStatus('disconnected');
    console.log('[WebSocket] Disconnected from project:', projectId);
  };

  const onEvent = (callback: (event: BoardEvent) => void) => {
    eventCallbacks.push(callback);

    // Return cleanup function
    return () => {
      eventCallbacks = eventCallbacks.filter((cb) => cb !== callback);
    };
  };

  return {
    connect,
    disconnect,
    status,
    onEvent,
  };
}

/**
 * SolidJS hook for WebSocket connection with automatic cleanup
 */
export function useWebSocket(projectId: string | undefined, baseUrl?: string) {
  if (!projectId) {
    return null;
  }

  const manager = createWebSocketManager(projectId, baseUrl);

  // Auto-connect
  manager.connect();

  // Auto-disconnect on cleanup
  onCleanup(() => {
    manager.disconnect();
  });

  return manager;
}
