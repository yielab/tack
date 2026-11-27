import { type Component, Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import { Icons, TypeTile, priorityLabel } from '../../shared/ui';
import ItemMenu from '../../shared/ui/ItemMenu';
import KeyLabel from '../../shared/ui/KeyLabel';
import type { Item, Priority, UpdateItem } from '../../shared/types';
import { shortItemId } from '../../shared/shortItemId';
import { FolderMark } from './FolderMark';
import type { FolderMarks } from './useFolderMarks';
import { PRIORITIES } from './ItemProperties';
import StatusMenu from './StatusMenu';

export interface ItemTitleProps {
  item: Item;
  /** Apply a partial update (optimistic PATCH handled by the parent). */
  onPatch: (patch: UpdateItem) => void;
  marks: FolderMarks;
  /** `drawer`: the status, priority and assignee stand under the title as chips; on the page they are in the Properties column. */
  layout: 'page' | 'drawer';
  /** Put the keyboard on the title when it appears (the page; the drawer focuses itself). */
  focusTitle?: boolean;
}

/** What an item is: its type, id and creation date, its title (editable), its key in the project folder and, in the drawer, its status chips. */
const ItemTitle: Component<ItemTitleProps> = (props) => {
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

  return (
    <div class="space-y-3">
      <div class="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-small" style={{ color: 'var(--color-text-secondary)' }}>
        <TypeTile type={props.item.item_type} />
        <span class="font-mono text-caption">{shortItemId(props.item.id)}</span>
        <span aria-hidden="true">·</span>
        <span>
          Created <time datetime={props.item.created_at}>{new Date(props.item.created_at).toLocaleDateString()}</time>
        </span>
      </div>

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
        class={`font-heading -mx-2 w-[calc(100%+1rem)] resize-none wrap-anywhere overflow-y-auto rounded-md border-2 border-transparent px-2 py-1 transition-colors hover:border-[var(--color-border-light)] focus:outline-none focus-visible:ring-2 ${props.layout === 'page' ? 'text-title' : 'text-step'}`}
        style={{
          'background-color': 'transparent',
          color: 'var(--color-text-primary)',
          'max-height': 'calc(3 * 1.25em + 0.5rem)',
          '--tw-ring-color': 'var(--color-focus-ring)',
        }}
        aria-label="Item title"
      />
      <div class="flex flex-wrap items-center gap-x-3">
        <KeyLabel row={props.item} />
        <FolderMark marks={props.marks} fields={['title', 'type']} />
      </div>

      <Show when={props.layout === 'drawer'}>
        <div class="flex flex-wrap items-center gap-2" data-testid="item-chips">
          <StatusMenu item={props.item} onChange={(status) => props.onPatch({ status })} />
          <ItemMenu
            buttonLabel={`Priority: ${priorityLabel(props.item.priority)}. Change priority`}
            menuLabel="Priority"
            align="left"
            label={<span data-testid="item-priority">{priorityLabel(props.item.priority)}</span>}
            icon={<Icons.IconChevronDown size={14} />}
            entries={PRIORITIES.map((p) => ({
              label: p.label,
              current: p.value === props.item.priority,
              onSelect: () => {
                if (p.value !== props.item.priority) props.onPatch({ priority: p.value as Priority });
              },
            }))}
          />
          <Show when={props.item.assignee}>
            <span
              data-testid="item-assignee"
              class="inline-flex h-8 items-center rounded-md border px-3 text-small font-semibold"
              style={{ 'border-color': 'var(--color-border-strong)', color: 'var(--color-text-primary)' }}
            >
              {props.item.assignee}
            </span>
          </Show>
        </div>
      </Show>
    </div>
  );
};

export default ItemTitle;
