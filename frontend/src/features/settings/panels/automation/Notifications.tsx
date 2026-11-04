import { type Component, For, Show, createResource } from 'solid-js';
import { api } from '../../../../shared/api';
import type { NotificationChannel } from '../../../../shared/api/notifications';
import { LoadError } from '../../../../shared/ui';
import { Block, Note } from './parts';

const LABELS: Record<string, string> = { webhook: 'Webhook' };
const label = (name: string) => LABELS[name] ?? name;

/** One channel: what waits for its next try, what was given up on, and why the last one failed. */
export function channelLine(c: NotificationChannel): string {
  const counts = `${c.pending} waiting, ${c.failed} given up`;
  return c.last_error ? `${counts}. Last error: ${c.last_error}` : counts;
}

/**
 * Notifications: the channels what needs a person leaves through besides this screen, one read-only
 * line each. A channel is set where Tack starts (the webhook's address and secret), not here.
 */
const Notifications: Component = () => {
  const [list, { refetch }] = createResource(() => api.notifications.channels());
  const channels = (): NotificationChannel[] => (list.error || !Array.isArray(list()) ? [] : list()!);

  return (
    <Block testid="notification-channels">
      <Show when={list.error}>
        {(error) => <LoadError what="the notification channels" error={error()} onRetry={() => void refetch()} />}
      </Show>
      <Show when={!list.error && !list.loading && channels().length === 0}>
        <Note>No channel is set. Set a webhook address to send what needs you to a receiver of your choice.</Note>
      </Show>
      <For each={channels()}>
        {(channel) => (
          <p class="text-sm" data-testid="notification-channel" data-channel={channel.name} style={{ color: 'var(--color-text-primary)' }}>
            <span class="font-medium">{label(channel.name)}</span>
            {' · '}
            <span style={{ color: channel.failed > 0 ? 'var(--color-danger-600)' : 'var(--color-text-secondary)' }}>{channelLine(channel)}</span>
          </p>
        )}
      </For>
    </Block>
  );
};

export default Notifications;
