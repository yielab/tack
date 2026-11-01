import { type Component, type JSX, createSignal, createResource, createUniqueId, For, Show } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import clsx from 'clsx';
import { Field, FieldShell, Select, Badge, Icons } from '../../shared/ui';
import { useVocab } from '../../shared/vocab/useVocab';
import { api } from '../../shared/api';
import type { Item, UpdateItem, Priority, Recurrence } from '../../shared/types';
import { estimateUnitLabel } from '../../shared/estimateUnit';

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
  /** `grid`: a compact list of label and value, as the drawer lays it out. `list`: the page's side column, each field under its label. */
  layout: 'grid' | 'list';
  /** How many items this one blocks, once the dependencies are loaded. */
  blocks?: number;
  /** Show the list of what it blocks. */
  onOpenBlocks?: () => void;
}

/**
 * One property: its label and its control. In the compact list the label sits left of the value;
 * in the side column it sits above it. The label is a real `<label>` for the control either way.
 */
const Prop: Component<{ label: string; id: string; compact: boolean; children: JSX.Element }> = (props) => (
  <Show
    when={props.compact}
    fallback={
      <FieldShell label={props.label} for={props.id}>
        {props.children}
      </FieldShell>
    }
  >
    <div class="contents">
      <dt class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
        <label for={props.id}>{props.label}</label>
      </dt>
      <dd class="min-w-0">{props.children}</dd>
    </div>
  </Show>
);

/** A read-only property: the label of a field, with a value that is not an input. */
const Fact: Component<{ label: string; compact: boolean; children: JSX.Element }> = (props) => (
  <Show
    when={props.compact}
    fallback={
      <FieldShell label={props.label}>
        <div class="text-sm" style={{ color: 'var(--color-text-primary)' }}>{props.children}</div>
      </FieldShell>
    }
  >
    <div class="contents">
      <dt class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>{props.label}</dt>
      <dd class="min-w-0 text-sm" style={{ color: 'var(--color-text-primary)' }}>{props.children}</dd>
    </div>
  </Show>
);

/** The item's tags as removable badges, with a field that adds one on Enter. */
const Tags: Component<{ item: Item; onPatch: (patch: UpdateItem) => void; compact: boolean }> = (props) => {
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

  const chips = (
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
  );
  return (
    <Show
      when={props.compact}
      fallback={
        <div>
          <p class="mb-1.5 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            Tags
          </p>
          {chips}
        </div>
      }
    >
      <div class="contents">
        <dt class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>Tags</dt>
        <dd class="min-w-0">{chips}</dd>
      </div>
    </Show>
  );
};

/** Every field of an item as one list; the parent picks the layout. Each commit calls `onPatch`. */
const ItemProperties: Component<ItemPropertiesProps> = (props) => {
  const vocab = useVocab();

  const [sprints] = createResource(
    () => props.item.project_id,
    (id) => api.sprints.list(id),
  );

  const compact = () => props.layout === 'grid';
  const idBase = createUniqueId();
  const id = (name: string) => `${idBase}-${name}`;

  return (
    <section aria-label="Properties">
      <Dynamic
        component={compact() ? 'dl' : 'div'}
        class={clsx(compact() ? 'grid items-center gap-x-4 gap-y-2.5' : 'flex flex-col gap-3.5')}
        style={compact() ? { 'grid-template-columns': 'minmax(5.5rem, auto) minmax(0, 1fr)' } : undefined}
      >
        <Prop label="Priority" id={id('priority')} compact={compact()}>
          <Select
            id={id('priority')}
            value={props.item.priority}
            onChange={(e) => props.onPatch({ priority: e.currentTarget.value as Priority })}
            options={PRIORITIES}
          />
        </Prop>

        <Prop label="Due date" id={id('due')} compact={compact()}>
          <Field
            id={id('due')}
            type="date"
            value={props.item.due_date ? props.item.due_date.split('T')[0] : ''}
            onChange={(e) => props.onPatch({ due_date: e.currentTarget.value || undefined })}
          />
        </Prop>

        <Prop label={vocab.t('assignee')} id={id('assignee')} compact={compact()}>
          <Field
            id={id('assignee')}
            type="text"
            value={props.item.assignee ?? ''}
            onChange={(e) => props.onPatch({ assignee: e.currentTarget.value.trim() || null })}
          />
        </Prop>

        <Prop
          label={`Estimate${props.item.estimate_unit ? ` (${estimateUnitLabel(props.item.estimate_unit)})` : ''}`}
          id={id('estimate')}
          compact={compact()}
        >
          <Field
            id={id('estimate')}
            type="number"
            min="0"
            value={props.item.estimate ?? ''}
            onChange={(e) => {
              const v = e.currentTarget.value;
              props.onPatch({ estimate: v === '' ? undefined : Number(v) });
            }}
          />
        </Prop>

        <Prop label={vocab.t('sprint')} id={id('sprint')} compact={compact()}>
          <Select
            id={id('sprint')}
            value={props.item.sprint_id ?? ''}
            onChange={(e) => props.onPatch({ sprint_id: e.currentTarget.value || undefined })}
          >
            <option value="">— None —</option>
            <For each={sprints() ?? []}>
              {(s) => <option value={s.id}>{s.name}</option>}
            </For>
          </Select>
        </Prop>

        <Prop label="Repeats" id={id('repeats')} compact={compact()}>
          <Select
            id={id('repeats')}
            value={props.item.recurrence ?? ''}
            onChange={(e) =>
              props.onPatch({ recurrence: (e.currentTarget.value || null) as Recurrence | null })
            }
            options={RECURRENCES}
          />
        </Prop>

        <Fact label="Last update" compact={compact()}>
          <time datetime={props.item.updated_at}>{new Date(props.item.updated_at).toLocaleString()}</time>
        </Fact>

        <Tags item={props.item} onPatch={props.onPatch} compact={compact()} />

        <Fact label="Blocks" compact={compact()}>
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
      </Dynamic>
    </section>
  );
};

export default ItemProperties;
