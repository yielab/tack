import { A } from '@solidjs/router';
import { type Component, For, Show } from 'solid-js';
import Drawer from '../../shared/ui/Drawer';
import { Badge, Button } from '../../shared/ui';
import { relativeTimeFromIso } from '../../shared/runWithAgent/shared';
import type { InboxNotification } from '../../shared/api/notifications';
import { notificationHref } from '../../shared/state/waiting';
import {
  closeInbox,
  inboxOpen,
  openRows,
  readAllNotifications,
  readNotification,
  recentRows,
  unreadCount,
} from '../../shared/state/notifications';

const Row: Component<{ n: InboxNotification }> = (p) => {
  const href = () => notificationHref(p.n);
  const follow = () => {
    if (!p.n.read_at) void readNotification(p.n.id);
    closeInbox();
  };
  const body = (
    <>
      <span class="block text-body font-medium" style={{ color: 'var(--color-text-primary)' }}>{p.n.title}</span>
      <span class="block text-small" style={{ color: 'var(--color-text-secondary)' }}>
        {p.n.cause} · {relativeTimeFromIso(p.n.created_at)}
      </span>
    </>
  );
  return (
    <li
      data-testid="inbox-row"
      data-level={p.n.level}
      class="flex items-start gap-3 py-3"
      style={{ 'border-bottom': '1px solid var(--color-border-light)' }}
    >
      <div class="min-w-0 flex-1">
        <Show when={href()} fallback={<div>{body}</div>}>
          <A href={href()!} onClick={follow} class="block rounded-md focus:outline-none focus-visible:ring-2" style={{ '--tw-ring-color': 'var(--color-focus-ring)' }}>
            {body}
          </A>
        </Show>
      </div>
      <Badge tone={p.n.level === 'needs_you' ? 'warning' : 'neutral'}>{p.n.level === 'needs_you' ? 'Needs you' : 'Info'}</Badge>
      <Show when={!p.n.read_at}>
        <Button
          size="sm"
          variant="ghost"
          aria-label={`Mark "${p.n.title}" read`}
          onClick={() => void readNotification(p.n.id)}
        >
          Read
        </Button>
      </Show>
    </li>
  );
};

const Section: Component<{ id: string; heading: string; rows: InboxNotification[]; empty?: string }> = (p) => (
  <section aria-labelledby={`inbox-${p.id}`} class="mb-8">
    <h3 id={`inbox-${p.id}`} class="mb-4 text-lead" style={{ color: 'var(--color-text-primary)' }}>
      {p.heading}
    </h3>
    <Show
      when={p.rows.length > 0}
      fallback={p.empty ? <p data-testid={`inbox-${p.id}-empty`} class="text-body" style={{ color: 'var(--color-text-secondary)' }}>{p.empty}</p> : null}
    >
      <ul class="flex flex-col" style={{ 'border-top': '1px solid var(--color-border-light)' }}>
        <For each={p.rows}>{(n) => <Row n={n} />}</For>
      </ul>
    </Show>
  </section>
);

/** What needs the person, then what happened lately, from the server's inbox. */
const InboxDrawer: Component = () => (
  <Drawer isOpen={inboxOpen()} onClose={closeInbox} title="Inbox" width="md">
    <div class="mb-8 flex items-center justify-end">
      <Button
        size="sm"
        variant="secondary"
        disabled={unreadCount() === 0}
        reason="Nothing unread"
        onClick={() => void readAllNotifications()}
      >
        Read all
      </Button>
    </div>
    <Section id="open" heading="Open" rows={openRows()} empty="Nothing needs you." />
    <Section id="recent" heading="Recent" rows={recentRows()} />
  </Drawer>
);

export default InboxDrawer;
