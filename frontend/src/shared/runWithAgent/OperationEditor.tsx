import { type Component, For, Show } from 'solid-js';
import type { Operation } from '../api/changeSets';
import { type Offer, inputStyle, withCurrent } from './PlanTree';
import WhoDoesIt from './WhoDoesIt';
import { PRIORITIES, priorityLabel, type OperationOf } from './changeSetOps';
import { teamOf } from '../execution/api';

/**
 * The fields of one operation that is not a create. A create has the tree; the six others have
 * a few fields each: who does it (role and team), a priority, a status, a comment, a due date. A
 * dependency has nothing to change, only to leave out. Every edit goes out through `onEdit` as the
 * whole operation, which is what the server replaces the waiting one with.
 */
const OperationEditor: Component<{
  op: Operation;
  /** The name the operation's item is shown under, for the accessible names. */
  name: string;
  offer: Offer;
  disabled: boolean;
  onEdit: (next: Operation) => void;
}> = (props) => {
  const o = <K extends Operation['op']>() => props.op as OperationOf<K>;
  const doer = () => {
    const op = o<'who_does_it'>();
    return op.team ?? (op.profile_id ? teamOf(props.offer.profiles.find((p) => p.agent_profile_id === op.profile_id)) : null);
  };
  return (
    <>
      <Show when={props.op.op === 'who_does_it'}>
        <div class="flex min-w-0 flex-wrap items-center gap-2 text-xs">
          <select
            aria-label={`Role for ${props.name}`}
            class="max-w-full rounded px-2 py-1 text-xs"
            style={inputStyle}
            disabled={props.disabled}
            onChange={(e) => props.onEdit({ ...o<'who_does_it'>(), role: e.currentTarget.value || null })}
          >
            <option value="" selected={!o<'who_does_it'>().role}>
              No role
            </option>
            <For each={withCurrent(props.offer.roles, o<'who_does_it'>().role ?? null)}>
              {(role) => (
                <option value={role.value} selected={o<'who_does_it'>().role === role.value}>
                  {role.label}
                </option>
              )}
            </For>
          </select>
          <span class="inline-flex max-w-full items-center rounded-full px-1" style={{ ...inputStyle, 'background-color': 'var(--color-bg-subtle)' }}>
            <WhoDoesIt
              label={`Who does it for ${props.name}`}
              value={doer()}
              profiles={props.offer.profiles}
              teams={props.offer.teams}
              disabled={props.disabled}
              onChange={(team) => props.onEdit({ ...o<'who_does_it'>(), team, profile_id: null })}
            />
          </span>
        </div>
      </Show>
      <Show when={props.op.op === 'priority'}>
        <select
          aria-label={`Priority of ${props.name}`}
          class="max-w-full rounded px-2 py-1 text-xs"
          style={inputStyle}
          disabled={props.disabled}
          onChange={(e) => props.onEdit({ ...o<'priority'>(), priority: e.currentTarget.value as (typeof PRIORITIES)[number] })}
        >
          <For each={PRIORITIES}>
            {(p) => (
              <option value={p} selected={o<'priority'>().priority === p}>
                {priorityLabel(p)}
              </option>
            )}
          </For>
        </select>
      </Show>
      <Show when={props.op.op === 'move'}>
        <Show
          when={props.offer.statuses.length > 0}
          fallback={
            <input
              aria-label={`Status of ${props.name}`}
              class="max-w-full rounded px-2 py-1 text-xs"
              style={inputStyle}
              disabled={props.disabled}
              value={o<'move'>().status}
              onChange={(e) => props.onEdit({ ...o<'move'>(), status: e.currentTarget.value })}
            />
          }
        >
          <select
            aria-label={`Status of ${props.name}`}
            class="max-w-full rounded px-2 py-1 text-xs"
            style={inputStyle}
            disabled={props.disabled}
            onChange={(e) => props.onEdit({ ...o<'move'>(), status: e.currentTarget.value })}
          >
            <For each={withCurrent(props.offer.statuses.map((s) => ({ value: s, label: s })), o<'move'>().status)}>
              {(s) => (
                <option value={s.value} selected={o<'move'>().status === s.value}>
                  {s.label}
                </option>
              )}
            </For>
          </select>
        </Show>
      </Show>
      <Show when={props.op.op === 'comment'}>
        <textarea
          aria-label={`Text of the comment on ${props.name}`}
          class="w-full min-w-0 rounded px-2 py-1 text-sm"
          style={inputStyle}
          rows={Math.max(2, o<'comment'>().text.split('\n').length)}
          disabled={props.disabled}
          value={o<'comment'>().text}
          onInput={(e) => props.onEdit({ ...o<'comment'>(), text: e.currentTarget.value })}
        />
      </Show>
      <Show when={props.op.op === 'due'}>
        <input
          type="date"
          aria-label={`Due date of ${props.name}`}
          class="max-w-full rounded px-2 py-1 text-xs"
          style={inputStyle}
          disabled={props.disabled}
          value={o<'due'>().due?.slice(0, 10) ?? ''}
          onChange={(e) => props.onEdit({ ...o<'due'>(), due: e.currentTarget.value ? `${e.currentTarget.value}T00:00:00Z` : null })}
        />
      </Show>
      <Show when={props.op.op === 'depend'}>
        <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          Nothing to change here: leave it out by unticking it.
        </p>
      </Show>
    </>
  );
};

export default OperationEditor;
