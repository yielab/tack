import { type Component, For, Show, createResource } from 'solid-js';
import { A } from '@solidjs/router';
import { api } from '../../shared/api';
import type { MessageRef, ThreadMessage } from '../../shared/api/threads';
import ChangeSetPanel from '../../shared/runWithAgent/ChangeSetPanel';
import { relativeTimeFromIso } from '../../shared/runWithAgent/shared';
import { shortItemId } from '../../shared/shortItemId';
import { refreshChangeSets } from '../../shared/state/waiting';
import { type Entry, writerName } from './threadModel';

/** A change set a reply points at, in the one panel that applies it. */
const InlineChangeSet: Component<{ id: string }> = (props) => {
  const [set] = createResource(
    () => props.id,
    (id) => api.changeSets.get(id),
  );
  return (
    <div data-testid="message-change-set" class="min-w-0">
      <Show
        when={!set.error}
        fallback={
          <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            The change set this answer made could not be read. Try the project's Change sets page.
          </p>
        }
      >
        <Show when={set()} fallback={<p class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>Reading the change set…</p>}>
          {(cs) => <ChangeSetPanel changeSet={cs()} onChange={() => void refreshChangeSets()} />}
        </Show>
      </Show>
    </div>
  );
};

/** What a message points at that is not a change set: an item to open, or a question. */
const RefLine: Component<{ r: MessageRef; projectId: string }> = (props) => (
  <Show
    when={props.r.kind === 'item'}
    fallback={
      <span class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
        About a question ({shortItemId(props.r.id)})
      </span>
    }
  >
    <A
      href={`/projects/${props.projectId}/items/${props.r.id}`}
      class="text-xs underline"
      style={{ color: 'var(--color-text-primary)' }}
    >
      Item {shortItemId(props.r.id)}
    </A>
  </Show>
);

/**
 * One message: who wrote it and when, its words, and what it points at. A reply carries the change sets
 * the turn made, each in the one panel that applies them. The reason a turn failed is said as that.
 */
const Message: Component<{ entry: Entry; profileName: string; projectId: string }> = (props) => {
  const message = (): ThreadMessage => props.entry.message;
  const mine = () => message().principal.startsWith('operator:');
  const failed = () => props.entry.failedTurn !== null;
  const sets = () => message().refs.filter((r) => r.kind === 'change_set');
  const others = () => message().refs.filter((r) => r.kind !== 'change_set');
  return (
    <li
      data-testid="thread-message"
      data-writer={mine() ? 'you' : 'agent'}
      data-failed={failed() ? 'true' : undefined}
      class="flex min-w-0 flex-col gap-2 rounded-item p-3"
      style={{
        border: `1px solid ${failed() ? 'var(--color-danger-600)' : 'var(--color-border-light)'}`,
        'background-color': mine() ? 'var(--color-accent-soft)' : 'transparent',
      }}
    >
      <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
        <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          {writerName(message().principal, props.profileName)}
        </span>{' '}
        · {relativeTimeFromIso(message().created_at)}
        <Show when={failed()}> · This turn failed</Show>
      </p>
      <p class="whitespace-pre-wrap text-sm" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>
        {message().body}
      </p>
      <Show when={others().length > 0}>
        <div class="flex flex-wrap gap-3">
          <For each={others()}>{(r) => <RefLine r={r} projectId={props.projectId} />}</For>
        </div>
      </Show>
      <For each={sets()}>{(r) => <InlineChangeSet id={r.id} />}</For>
    </li>
  );
};

export default Message;
