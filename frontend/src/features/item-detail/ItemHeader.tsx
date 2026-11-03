import { type Component, type JSX, createResource, createSignal, createEffect, onCleanup, onMount, For, Show } from 'solid-js';
import { api } from '../../shared/api';
import { A } from '@solidjs/router';
import { Icons, TypeBadge, typeKey } from '../../shared/ui';
import { statusDotColor } from '../../shared/ui/statusDot';
import ItemMenu from './ItemMenu';
import { useProjectItems } from '../../shared/state/projectItemsContext';
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
  // The project's items name the parent and the sub-items. A screen under the board's tabs already has them;
  // the drawer sits outside those tabs, so it reads them itself rather than name nobody.
  const shared = (() => {
    try {
      return useProjectItems().items;
    } catch {
      return null;
    }
  })();
  const [own] = createResource(
    () => (shared ? null : `${props.item.project_id}:${props.item.updated_at}`),
    () => api.items.list(props.item.project_id).catch(() => [] as Item[]),
  );
  const projectItems = () => (shared ? shared() : own());
  const parent = () => (props.item.parent_id ? projectItems()?.find((i) => i.id === props.item.parent_id) : undefined);
  const children = () => projectItems()?.filter((i) => i.parent_id === props.item.id) ?? [];
  const href = (id: string) => `/projects/${props.item.project_id}/items/${id}`;

  // Title is locally mirrored so it commits on blur / Enter, not every keypress.
  const [title, setTitle] = createSignal(props.item.title);
  createEffect(() => setTitle(props.item.title));
  // A pasted title can carry line breaks; a title is one line, so they become spaces on commit.
  const commitTitle = () => {
    const next = title().replace(/\s*\n\s*/g, ' ').trim();
    if (next && next !== props.item.title) props.onPatch({ title: next });
  };

  // The title wraps, which an input cannot do: the field grows with its text to three
  // lines (see max-height below) and scrolls inside itself past that.
  let titleInput: HTMLTextAreaElement | undefined;
  const fitTitle = () => {
    if (!titleInput) return;
    titleInput.style.height = 'auto';
    titleInput.style.height = `${titleInput.scrollHeight}px`;
  };
  createEffect(() => { title(); fitTitle(); });
  onMount(() => {
    if (props.focusTitle) titleInput?.focus({ preventScroll: true });
    window.addEventListener('resize', fitTitle);
  });
  onCleanup(() => window.removeEventListener('resize', fitTitle));

  const statuses = () => workflow()?.statuses ?? [];
  const categoryOf = (status: string) => statuses().find((s) => s.name === status)?.category;
  const StatusDot: Component<{ category: string | undefined }> = (p) => (
    <span aria-hidden="true" class="flex-none" style={{ width: '8px', height: '8px', 'border-radius': '50%', background: statusDotColor(p.category) }} />
  );

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
      <textarea
        ref={titleInput}
        rows={1}
        value={title()}
        title={props.item.title}
        onInput={(e) => setTitle(e.currentTarget.value)}
        onBlur={commitTitle}
        onKeyDown={(e) => {
          // Enter commits the title, as it did when the title was a single-line input.
          if (e.key === 'Enter') {
            e.preventDefault();
            e.currentTarget.blur();
          }
        }}
        class="font-heading -mx-2 w-[calc(100%+1rem)] resize-none wrap-anywhere overflow-y-auto rounded-item border-2 border-transparent px-2 py-1 text-[1.875rem] leading-tight transition-colors hover:border-[var(--color-border-light)] focus:outline-none focus-visible:ring-2"
        style={{
          'background-color': 'transparent',
          color: 'var(--color-text-primary)',
          'max-height': 'calc(3 * 1.25em + 0.5rem)',
          '--tw-ring-color': 'var(--color-focus-ring)',
        }}
        aria-label="Item title"
      />

      {/* Status: a person's. What the agents report is Work, in the Work panel below. Only the current
          status shows; the others are options behind one button, never a row of steps. */}
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-xs font-semibold" style={{ color: 'var(--color-text-tertiary)' }}>Status</span>
        <span
          data-testid="item-status"
          class="inline-flex items-center gap-2 rounded-full px-3 py-1 text-sm font-semibold"
          style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)' }}
        >
          <StatusDot category={categoryOf(props.item.status)} />
          {props.item.status}
        </span>
        <ItemMenu
          buttonLabel="Change status"
          menuLabel="Status"
          icon={<Icons.IconChevronDown size={14} />}
          entries={statuses().map((s) => ({
            label: s.name,
            current: s.name === props.item.status,
            leading: <StatusDot category={s.category} />,
            onSelect: () => { if (s.name !== props.item.status) props.onPatch({ status: s.name }); },
          }))}
        />
      </div>

      <Show when={parent()}>
        {(p) => (
          <p data-testid="item-parent" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            Part of:{' '}
            <A href={href(p().id)} class="font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }}>
              {p().title}
            </A>
          </p>
        )}
      </Show>
      <Show when={children().length > 0}>
        <div data-testid="item-children" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          <span>Sub-items:</span>
          <ul class="mt-0.5 flex flex-col gap-0.5">
            <For each={children()}>
              {(c) => (
                <li class="min-w-0">
                  <A href={href(c.id)} class="font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }}>
                    {c.title}
                  </A>
                  <span style={{ color: 'var(--color-text-tertiary)' }}> · {c.status}</span>
                </li>
              )}
            </For>
          </ul>
        </div>
      </Show>
    </div>
  );
};

export default ItemHeader;
