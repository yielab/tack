import { type Component, type JSX, createSignal, createResource, For, Show } from 'solid-js';
import clsx from 'clsx';
import { Field, FieldShell, Select, Badge, Icons } from '../../shared/ui';
import { useVocab } from '../../shared/vocab/useVocab';
import { api } from '../../shared/api';
import type { Item, UpdateItem, Priority, Recurrence } from '../../shared/types';
import { estimateUnitLabel } from '../../shared/estimateUnit';
import AutomationRow from './AutomationRow';

const PRIORITIES: { value: Priority; label: string }[] = [
  { value: 'critical', label: '🔥 Critical' },
  { value: 'high', label: '⬆️ High' },
  { value: 'medium', label: '➡️ Medium' },
  { value: 'low', label: '⬇️ Low' },
  { value: 'none', label: '➖ None' },
];

const RECURRENCES: { value: '' | Recurrence; label: string }[] = [
  { value: '', label: 'Never' },
  { value: 'weekly', label: 'Weekly' },
  { value: 'monthly', label: 'Monthly' },
  { value: 'quarterly', label: 'Quarterly' },
];

export interface ItemPropertiesProps {
  item: Item;
  /** Apply a partial update (optimistic PATCH handled by the parent). */
  onPatch: (patch: UpdateItem) => void;
  /** `grid`: two columns, as the drawer lays it out. `list`: one column, the page's side column. */
  layout: 'grid' | 'list';
  /** How many items this one blocks, once the dependencies are loaded. */
  blocks?: number;
  /** Show the list of what it blocks. */
  onOpenBlocks?: () => void;
}

/** A read-only property: the label of a field, with a value that is not an input. */
const Fact: Component<{ label: string; children: JSX.Element }> = (props) => (
  <FieldShell label={props.label} class="col-span-full">
    <div class="text-sm" style={{ color: 'var(--color-text-primary)' }}>{props.children}</div>
  </FieldShell>
);

/** Every field of an item as one list; the parent picks the layout. Each commit calls `onPatch`. */
const ItemProperties: Component<ItemPropertiesProps> = (props) => {
  const vocab = useVocab();

  const [tagInput, setTagInput] = createSignal('');
  const addTag = () => {
    const tag = tagInput().trim();
    if (tag && !props.item.tags.includes(tag)) {
      props.onPatch({ tags: [...props.item.tags, tag] });
    }
    setTagInput('');
  };
  const removeTag = (tag: string) =>
    props.onPatch({ tags: props.item.tags.filter((t) => t !== tag) });

  const [sprints] = createResource(
    () => props.item.project_id,
    (id) => api.sprints.list(id),
  );

  return (
    <section
      aria-label="Properties"
      class={clsx(
        props.layout === 'grid' ? 'grid grid-cols-2 gap-x-[18px] gap-y-3.5' : 'flex flex-col gap-3.5',
      )}
    >
      <Select
        label="Priority"
        value={props.item.priority}
        onChange={(e) => props.onPatch({ priority: e.currentTarget.value as Priority })}
        options={PRIORITIES}
      />

      <Field
        label="Due date"
        type="date"
        value={props.item.due_date ? props.item.due_date.split('T')[0] : ''}
        onChange={(e) => props.onPatch({ due_date: e.currentTarget.value || undefined })}
      />

      <Field
        label={vocab.t('assignee')}
        type="text"
        value={props.item.assignee ?? ''}
        onChange={(e) => props.onPatch({ assignee: e.currentTarget.value.trim() || null })}
      />

      <Field
        label={`Estimate${props.item.estimate_unit ? ` (${estimateUnitLabel(props.item.estimate_unit)})` : ''}`}
        type="number"
        min="0"
        value={props.item.estimate ?? ''}
        onChange={(e) => {
          const v = e.currentTarget.value;
          props.onPatch({ estimate: v === '' ? undefined : Number(v) });
        }}
      />

      <Select
        label={vocab.t('sprint')}
        value={props.item.sprint_id ?? ''}
        onChange={(e) => props.onPatch({ sprint_id: e.currentTarget.value || undefined })}
      >
        <option value="">— None —</option>
        <For each={sprints() ?? []}>
          {(s) => <option value={s.id}>{s.name}</option>}
        </For>
      </Select>

      <Select
        label="Repeats"
        value={props.item.recurrence ?? ''}
        onChange={(e) =>
          props.onPatch({ recurrence: (e.currentTarget.value || null) as Recurrence | null })
        }
        options={RECURRENCES}
      />

      <Fact label="Last update">
        <time datetime={props.item.updated_at}>{new Date(props.item.updated_at).toLocaleString()}</time>
      </Fact>

      <div class="col-span-full">
        <p class="mb-1.5 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          Tags
        </p>
        <div class="flex flex-wrap items-center gap-1.5">
          <For each={props.item.tags}>
            {(tag) => (
              <Badge tone="primary">
                <span class="flex items-center gap-1">
                  {tag}
                  <button
                    type="button"
                    aria-label={`Remove ${tag}`}
                    onClick={() => removeTag(tag)}
                    class="grid place-items-center rounded-full leading-none hover:opacity-70 focus:outline-none focus-visible:ring-2"
                    style={{ '--tw-ring-color': 'var(--color-focus-ring)' }}
                  >
                    <Icons.IconClose size={10} stroke-width={2.4} />
                  </button>
                </span>
              </Badge>
            )}
          </For>
          <input
            aria-label="Add tag"
            value={tagInput()}
            onInput={(e) => setTagInput(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                addTag();
              }
            }}
            placeholder="Add tag…"
            class="min-w-[6rem] flex-1 rounded-full border px-3.5 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
            style={{
              'background-color': 'var(--color-bg-base)',
              color: 'var(--color-text-primary)',
              'border-color': 'var(--color-border-medium)',
              '--tw-ring-color': 'var(--color-focus-ring)',
            }}
          />
        </div>
      </div>

      <div class="col-span-full">
        <AutomationRow item={props.item} />
      </div>

      <Fact label="Blocks">
        <Show
          when={props.blocks}
          fallback={props.blocks === 0 ? 'Nothing' : '—'}
        >
          <button
            type="button"
            onClick={() => props.onOpenBlocks?.()}
            class="font-semibold hover:underline focus:outline-none focus-visible:ring-2"
            style={{ color: 'var(--color-accent-ink)', '--tw-ring-color': 'var(--color-focus-ring)' }}
          >
            {props.blocks} {props.blocks === 1 ? 'item' : 'items'}
          </button>
        </Show>
      </Fact>
    </section>
  );
};

export default ItemProperties;
