import { type Component, For, Show, createEffect, createMemo, createResource, createSignal, createUniqueId, onMount } from 'solid-js';
import clsx from 'clsx';
import { api } from '../api';
import { useItemTypes } from '../vocab/useItemTypes';
import type { ParentOption } from '../types';
import TypeTile from './TypeTile';

export interface ParentPickerProps {
  projectId: string;
  /** Whose parent is being chosen: an item that exists (it and what lies under it are left out), or the type key of one about to be made. */
  of: { item: string } | { type: string };
  /** The parent the item has now (null: none). It is marked and not offered again; leave it out when unknown. */
  current?: string | null;
  /** Offer "No parent", which puts the item at the top. */
  allowNone?: boolean;
  /** The chosen parent, or null for no parent. */
  onPick: (parent: ParentOption | null) => void;
  /** The field's accessible name; "Move under" by default. */
  label?: string;
  class?: string;
}

interface Choice {
  id: string;
  parent: ParentOption | null;
  text: string;
  current: boolean;
}

/**
 * The keyboard's way to move an item under another: a combobox over the parents the server admits
 * for it (`GET /api/projects/{id}/parents`), so the web offers exactly what the tree rule allows and
 * never works the rule out itself. Typing narrows by title or type, the arrow keys move, Enter picks.
 * The list is shown whole under the field, so it works inside a dialog without a popup of its own.
 */
const ParentPicker: Component<ParentPickerProps> = (props) => {
  const types = useItemTypes();
  const base = createUniqueId();
  const listId = `${base}-list`;
  const [query, setQuery] = createSignal('');
  const [active, setActive] = createSignal(0);
  let input: HTMLInputElement | undefined;

  const [parents, { refetch }] = createResource(
    () => [props.projectId, props.of] as const,
    ([projectId, of]) => api.items.parents(projectId, of),
  );

  // Reading a failed resource throws, so every read goes through this.
  const admitted = (): ParentOption[] => (parents.error ? [] : (parents() ?? []));

  const choices = createMemo((): Choice[] => {
    const needle = query().trim().toLowerCase();
    const list: Choice[] = [];
    if (props.allowNone !== false && (!needle || 'no parent'.includes(needle))) {
      list.push({ id: `${base}-none`, parent: null, text: 'No parent', current: props.current === null });
    }
    for (const parent of admitted()) {
      if (needle && !`${parent.title} ${types.name(parent.type)}`.toLowerCase().includes(needle)) continue;
      list.push({ id: `${base}-${parent.id}`, parent, text: parent.title, current: props.current === parent.id });
    }
    return list;
  });

  // The highlight starts on the first choice that can be picked and stays in range as the list narrows.
  createEffect(() => {
    const list = choices();
    setActive(Math.max(0, list.findIndex((c) => !c.current)));
  });

  onMount(() => setTimeout(() => input?.focus(), 0));

  const pick = (choice: Choice | undefined) => {
    if (choice && !choice.current) props.onPick(choice.parent);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const list = choices();
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (list.length > 0) setActive((i) => (i + (e.key === 'ArrowDown' ? 1 : -1) + list.length) % list.length);
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      setActive(e.key === 'Home' ? 0 : Math.max(0, list.length - 1));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      pick(list[active()]);
    }
  };

  return (
    <div data-testid="parent-picker" class={clsx('flex min-w-0 flex-col gap-2', props.class)}>
      <input
        ref={input}
        type="text"
        role="combobox"
        aria-label={props.label ?? 'Move under'}
        aria-expanded="true"
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={choices()[active()]?.id}
        placeholder="Search by title or type"
        value={query()}
        onInput={(e) => setQuery(e.currentTarget.value)}
        onKeyDown={onKeyDown}
        class="min-h-9 w-full rounded-full border px-3.5 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
        style={{
          'background-color': 'var(--color-bg-base)',
          color: 'var(--color-text-primary)',
          'border-color': 'var(--color-border-medium)',
          '--tw-ring-color': 'var(--color-focus-ring)',
        }}
      />
      <Show when={!parents.loading} fallback={<p role="status" class="m-0 text-xs" style={{ color: 'var(--color-text-secondary)' }}>Loading the places this can go…</p>}>
        <Show
          when={!parents.error}
          fallback={
            <p role="alert" class="m-0 flex items-center gap-2 text-xs" style={{ color: 'var(--color-danger-600)' }}>
              The places this can go did not load.
              <button type="button" onClick={() => void refetch()} class="rounded-full border px-2 py-0.5 text-xs font-semibold focus:outline-none focus-visible:ring-2" style={{ 'border-color': 'var(--color-border-strong)', color: 'var(--color-text-primary)', '--tw-ring-color': 'var(--color-focus-ring)' }}>
                Try again
              </button>
            </p>
          }
        >
          <ul
            id={listId}
            role="listbox"
            aria-label={props.label ?? 'Move under'}
            class="m-0 flex max-h-64 list-none flex-col gap-0.5 overflow-y-auto p-0"
          >
            <For each={choices()}>
              {(choice, i) => (
                <li
                  id={choice.id}
                  role="option"
                  aria-selected={active() === i()}
                  aria-disabled={choice.current ? 'true' : undefined}
                  onClick={() => pick(choice)}
                  onMouseMove={() => { if (!choice.current) setActive(i()); }}
                  class="flex min-h-9 items-center gap-2 rounded-item px-3 py-1.5 text-sm"
                  style={{
                    cursor: choice.current ? 'not-allowed' : 'pointer',
                    opacity: choice.current ? 0.55 : 1,
                    color: 'var(--color-text-primary)',
                    'background-color': active() === i() && !choice.current ? 'var(--color-bg-hover)' : 'transparent',
                  }}
                >
                  <Show when={choice.parent}>{(p) => <TypeTile type={p().type} class="flex-none" />}</Show>
                  <span class="min-w-0 flex-1 truncate" title={choice.text}>{choice.text}</span>
                  <Show when={choice.current}>
                    <span class="flex-none text-xs font-semibold" style={{ color: 'var(--color-text-secondary)' }}>Current</span>
                  </Show>
                </li>
              )}
            </For>
          </ul>
          <Show when={choices().length === 0}>
            <p role="status" class="m-0 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
              {admitted().length === 0 ? 'Nothing in this project can hold this item.' : 'Nothing matches that.'}
            </p>
          </Show>
        </Show>
      </Show>
    </div>
  );
};

export default ParentPicker;
