import { request } from './client';
import type { components } from './schema.gen';

type Schemas = components['schemas'];

/** A notice as the inbox keeps it (not the browser's `Notification`). */
export type InboxNotification = Schemas['Notification'];
export type NotificationKind = Schemas['Kind'];
export type NotificationLevel = Schemas['Level'];
export type NotificationList = Schemas['NotificationList'];

export const notifications = {
  /** `open` (the default): what still asks something of the person; `all`: everything kept, newest first. */
  list: (state: 'open' | 'all' = 'open', limit?: number) => {
    const query = new URLSearchParams({ state });
    if (limit !== undefined) query.set('limit', String(limit));
    return request<NotificationList>(`/notifications?${query.toString()}`);
  },
  read: (id: string) => request<void>(`/notifications/${encodeURIComponent(id)}/read`, { method: 'POST' }),
  readAll: () => request<{ read: number }>('/notifications/read-all', { method: 'POST' }),
};
