import type { InboxNotification } from '../api/notifications';
import { createReconnectingSocket, liveUrl, type ReconnectingSocket, type ReconnectingSocketOptions } from './socket';

/**
 * What the inbox's live socket says. The first message, and the one after the server's listener
 * fell behind, is just the count (`{ open }`): read the list afresh. Every other message is one
 * change with the count after it.
 */
export type NotificationMessage =
  | { type?: undefined; open: number }
  | ({ type: 'opened'; open: number } & InboxNotification)
  | { type: 'closed'; id: string; key: string; open: number }
  | { type: 'read'; id: string; open: number };

export function notificationsLiveUrl(): string {
  return liveUrl('/notifications/live');
}

export function createNotificationsSocket(
  onMessage: (message: NotificationMessage) => void,
  options: ReconnectingSocketOptions & { url?: string } = {}
): ReconnectingSocket {
  const { url = notificationsLiveUrl(), ...socketOptions } = options;
  return createReconnectingSocket(
    url,
    (message) => {
      if (message && typeof message === 'object' && typeof (message as { open?: unknown }).open === 'number') {
        onMessage(message as NotificationMessage);
      }
    },
    socketOptions
  );
}
