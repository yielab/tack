import { type Component, type JSX, createSignal, createEffect, onMount, For, Show } from 'solid-js';
import { TypeBadge, typeKey } from '../../shared/ui';
import { useProject } from '../../shared/state/projectContext';
import { useVocab } from '../../shared/vocab/useVocab';
import type { Item, UpdateItem } from '../../shared/types';
import { shortItemId } from '../../shared/shortItemId';

export interface ItemHeaderProps {
  item: Item;
  /** Apply a partial update (optimistic PATCH handled by the parent). */
  onPatch: (patch: UpdateItem) => void;
  /** Buttons at the right of the type row (Copy link on the page). */
  actions?: JSX.Element;
  /** Put the keyboard on the title when the header appears (the page; the drawer focuses itself). */
  focusTitle?: boolean;
}

/** What an item is: type, id, creation date, an editable title and its status. */
const ItemHeader: Component<ItemHeaderProps> = (props) => {
  const { workflow } = useProject();
  const vocab = useVocab();

  // Title is locally mirrored so it commits on blur / Enter, not every keypress.
  const [title, setTitle] = createSignal(props.item.title);
  createEffect(() => setTitle(props.item.title));
  const commitTitle = () => {
    const next = title().trim();
    if (next && next !== props.item.title) props.onPatch({ title: next });
  };

  let titleInput: HTMLInputElement | undefined;
  onMount(() => { if (props.focusTitle) titleInput?.focus({ preventScroll: true }); });

  const statuses = () => workflow()?.statuses ?? [];

  return (
    <div class="space-y-5">
      {/* Type + id */}
      <div class="flex flex-wrap items-center gap-2">
        <TypeBadge type={props.item.item_type} label={vocab.t(typeKey(props.item.item_type))} />
        <span style={{ 'font-family': 'var(--font-mono)', 'font-size': '0.75rem', color: 'var(--color-text-tertiary)' }}>
          {shortItemId(props.item.id)}
        </span>
        <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Created <time datetime={props.item.created_at}>{new Date(props.item.created_at).toLocaleDateString()}</time>
        </span>
        <Show when={props.actions}>
          <div class="ml-auto flex items-center gap-2">{props.actions}</div>
        </Show>
      </div>

      {/* Title */}
      <input
        ref={titleInput}
        value={title()}
        onInput={(e) => setTitle(e.currentTarget.value)}
        onBlur={commitTitle}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
        }}
        class="font-heading -mx-2 w-[calc(100%+1rem)] rounded-[20px] border-2 border-transparent px-2 py-1 text-[1.875rem] leading-tight transition-colors hover:border-[var(--color-border-light)] focus:outline-none focus-visible:ring-2"
        style={{
          'background-color': 'transparent',
          color: 'var(--color-text-primary)',
          '--tw-ring-color': 'var(--color-focus-ring)',
        }}
        aria-label="Item title"
      />

      {/* Status pills */}
      <div
        class="inline-flex max-w-full flex-wrap gap-1 rounded-[22px] p-1"
        style={{ 'background-color': 'var(--color-bg-panel)' }}
        role="group"
        aria-label="Status"
      >
        <For each={statuses()}>
          {(s) => {
            const active = () => props.item.status === s.name;
            return (
              <button
                type="button"
                onClick={() => { if (!active()) props.onPatch({ status: s.name }); }}
                aria-pressed={active() ? 'true' : 'false'}
                class="focus:outline-none focus-visible:ring-2"
                style={{
                  padding: '6px 14px', 'border-radius': '999px', cursor: 'pointer',
                  'font-size': '0.8125rem', 'font-weight': active() ? 600 : 500, 'font-family': 'inherit',
                  border: 'none',
                  background: active() ? 'var(--color-primary-600)' : 'transparent',
                  color: active() ? 'var(--color-on-accent)' : 'var(--color-text-secondary)',
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              >
                {s.name}
              </button>
            );
          }}
        </For>
      </div>

      <Show when={props.item.parent_id}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Has a parent item (re-parenting arrives with the tree view).
        </p>
      </Show>
    </div>
  );
};

export default ItemHeader;
