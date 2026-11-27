import { type Component, type JSX, createSignal, createResource, createUniqueId, For, Show } from 'solid-js';
import { Field, Select, Badge, Icons } from '../../shared/ui';
import { useVocab } from '../../shared/vocab/useVocab';
import { api } from '../../shared/api';
import type { Item, UpdateItem, Priority, Recurrence } from '../../shared/types';
import { estimateUnitLabel } from '../../shared/estimateUnit';
import { BoardMark } from './FolderMark';
import type { FolderMarks } from './useFolderMarks';
import StatusMenu from './StatusMenu';

export const PRIORITIES: { value: Priority; label: string }[] = [
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
  marks: FolderMarks;
  /** The page lists the status here; the drawer shows it as a chip under the title instead. */
  showStatus: boolean;
}

/** One property: its label (a real `<label>` for the control) at the left, its value at the right. `board`: the board owns this field. */
const Prop: Component<{ label: string; id?: string; board?: FolderMarks; children: JSX.Element }> = (props) => (
  <div class="contents">
    <dt class="pt-2 text-small" style={{ color: 'var(--color-text-secondary)' }}>
      <Show when={props.id} fallback={props.label}>
        <label for={props.id}>{props.label}</label>
      </Show>
      <Show when={props.board}>{(marks) => <div><BoardMark marks={marks()} /></div>}</Show>
    </dt>
    <dd class="min-w-0">{props.children}</dd>
  </div>
);

/** A read-only property: the label of a field, with a value that is not an input. */
const Fact: Component<{ label: string; children: JSX.Element }> = (props) => (
  <div class="contents">
    <dt class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{props.label}</dt>
    <dd class="min-w-0 text-body" style={{ color: 'var(--color-text-primary)' }}>{props.children}</dd>
  </div>
);

/** The item's tags as removable badges, with a field that adds one on Enter. */
const Tags: Component<{ item: Item; onPatch: (patch: UpdateItem) => void }> = (props) => {
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

  return (
    <div class="contents">
      <dt class="pt-2 text-small" style={{ color: 'var(--color-text-secondary)' }}>Tags</dt>
      <dd class="min-w-0">
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
                    class="grid place-items-center rounded-sm leading-none hover:opacity-70 focus:outline-none focus-visible:ring-2"
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
            class="min-w-24 flex-1 rounded-md border px-3 py-1.5 text-small focus:outline-none focus-visible:ring-2"
            style={{
              'background-color': 'var(--color-bg-base)',
              color: 'var(--color-text-primary)',
              'border-color': 'var(--color-border-medium)',
              '--tw-ring-color': 'var(--color-focus-ring)',
            }}
          />
        </div>
      </dd>
    </div>
  );
};

/** Every field of an item as one list; the parent picks the layout. Each commit calls `onPatch`. */
const ItemProperties: Component<ItemPropertiesProps> = (props) => {
  const vocab = useVocab();

  const [sprints] = createResource(
    () => props.item.project_id,
    (id) => api.sprints.list(id),
  );

  const idBase = createUniqueId();
  const id = (name: string) => `${idBase}-${name}`;

  return (
    <section aria-label="Properties">
      <dl class="grid items-start gap-x-4 gap-y-2" style={{ 'grid-template-columns': '6.5rem minmax(0, 1fr)' }}>
        <Show when={props.showStatus}>
          <Prop label="Status" board={props.marks}>
            <StatusMenu item={props.item} onChange={(status) => props.onPatch({ status })} />
          </Prop>
        </Show>

        <Prop label="Priority" id={id('priority')} board={props.marks}>
          <Select
            id={id('priority')}
            value={props.item.priority}
            onChange={(e) => props.onPatch({ priority: e.currentTarget.value as Priority })}
            options={PRIORITIES}
          />
        </Prop>

        <Prop label={vocab.t('assignee')} id={id('assignee')} board={props.marks}>
          <Field
            id={id('assignee')}
            type="text"
            value={props.item.assignee ?? ''}
            onChange={(e) => props.onPatch({ assignee: e.currentTarget.value.trim() || null })}
          />
        </Prop>

        <Prop label={vocab.t('sprint')} id={id('sprint')} board={props.marks}>
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

        <Prop label="Due date" id={id('due')} board={props.marks}>
          <Field
            id={id('due')}
            type="date"
            value={props.item.due_date ? props.item.due_date.split('T')[0] : ''}
            onChange={(e) => props.onPatch({ due_date: e.currentTarget.value || undefined })}
          />
        </Prop>

        <Tags item={props.item} onPatch={props.onPatch} />

        <Prop
          label={`Estimate${props.item.estimate_unit ? ` (${estimateUnitLabel(props.item.estimate_unit)})` : ''}`}
          id={id('estimate')}
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

        <Prop label="Repeats" id={id('repeats')}>
          <Select
            id={id('repeats')}
            value={props.item.recurrence ?? ''}
            onChange={(e) =>
              props.onPatch({ recurrence: (e.currentTarget.value || null) as Recurrence | null })
            }
            options={RECURRENCES}
          />
        </Prop>

        <Fact label="Last update">
          <time datetime={props.item.updated_at}>{new Date(props.item.updated_at).toLocaleString()}</time>
        </Fact>
      </dl>
    </section>
  );
};

export default ItemProperties;
