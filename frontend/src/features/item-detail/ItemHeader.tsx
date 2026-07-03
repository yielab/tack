import { type Component, createSignal, createEffect, For, Show } from 'solid-js';
import { createResource } from 'solid-js';
import { Field, Select, Badge, TypeBadge, typeKey } from '../../shared/ui';
import { useProject } from '../../shared/state/projectContext';
import { useVocab } from '../../shared/vocab/useVocab';
import { api } from '../../shared/api';
import type { Item, UpdateItem, Priority } from '../../shared/types';
import { estimateUnitLabel } from '../../shared/estimateUnit';

const shortId = (id: string) => id.replace(/-/g, '').slice(0, 6).toUpperCase();

const PRIORITIES: { value: Priority; label: string }[] = [
  { value: 'critical', label: '🔥 Critical' },
  { value: 'high', label: '⬆️ High' },
  { value: 'medium', label: '➡️ Medium' },
  { value: 'low', label: '⬇️ Low' },
  { value: 'none', label: '➖ None' },
];

export interface ItemHeaderProps {
  item: Item;
  /** Apply a partial update (optimistic PATCH handled by the parent). */
  onPatch: (patch: UpdateItem) => void;
}

/** Inline-editable core fields for an item. Each commit calls `onPatch`. */
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

  const statuses = () => workflow()?.statuses ?? [];

  return (
    <div class="space-y-4">
      {/* Type + id */}
      <div class="flex items-center gap-2">
        <TypeBadge type={props.item.item_type} label={vocab.t(typeKey(props.item.item_type))} />
        <span style={{ 'font-family': 'var(--font-mono)', 'font-size': '12px', color: 'var(--color-text-tertiary)' }}>
          {shortId(props.item.id)}
        </span>
      </div>

      {/* Title */}
      <input
        value={title()}
        onInput={(e) => setTitle(e.currentTarget.value)}
        onBlur={commitTitle}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
        }}
        class="w-full rounded-lg border border-transparent px-2 py-1 text-2xl font-bold transition-colors hover:border-[var(--color-border-light)] focus:outline-none focus-visible:ring-2"
        style={{
          'background-color': 'transparent',
          color: 'var(--color-text-primary)',
          '--tw-ring-color': 'var(--color-focus-ring)',
        }}
        aria-label="Item title"
      />

      {/* Status pills */}
      <div class="flex flex-wrap gap-1.5" role="group" aria-label="Status">
        <For each={statuses()}>
          {(s) => {
            const active = () => props.item.status === s.name;
            return (
              <button
                type="button"
                onClick={() => { if (!active()) props.onPatch({ status: s.name }); }}
                aria-pressed={active() ? 'true' : 'false'}
                style={{
                  padding: '5px 11px', 'border-radius': '8px', cursor: 'pointer',
                  'font-size': '12px', 'font-weight': 600, 'font-family': 'inherit',
                  border: '1px solid ' + (active() ? 'transparent' : 'var(--color-border-light)'),
                  background: active() ? 'var(--color-primary-600)' : 'var(--color-bg-base)',
                  color: active() ? 'var(--color-on-accent)' : 'var(--color-text-secondary)',
                }}
              >
                {s.name}
              </button>
            );
          }}
        </For>
      </div>

      {/* Field grid */}
      <div class="grid grid-cols-2 gap-3">
        <Select
          label="Priority"
          value={props.item.priority}
          onChange={(e) => props.onPatch({ priority: e.currentTarget.value as Priority })}
          options={PRIORITIES}
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

        <Field
          label="Due date"
          type="date"
          value={props.item.due_date ? props.item.due_date.split('T')[0] : ''}
          onChange={(e) =>
            props.onPatch({ due_date: e.currentTarget.value || undefined })
          }
        />

        <Select
          label={vocab.t('sprint')}
          value={props.item.sprint_id ?? ''}
          onChange={(e) =>
            props.onPatch({ sprint_id: e.currentTarget.value || undefined })
          }
        >
          <option value="">— None —</option>
          <For each={sprints() ?? []}>
            {(s) => <option value={s.id}>{s.name}</option>}
          </For>
        </Select>
      </div>

      {/* Tags */}
      <div>
        <p class="mb-1 text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
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
                    class="leading-none hover:opacity-70"
                  >
                    ×
                  </button>
                </span>
              </Badge>
            )}
          </For>
          <input
            value={tagInput()}
            onInput={(e) => setTagInput(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                addTag();
              }
            }}
            placeholder="Add tag…"
            class="min-w-[6rem] flex-1 rounded-md border px-2 py-1 text-sm focus:outline-none focus-visible:ring-2"
            style={{
              'background-color': 'var(--color-bg-base)',
              color: 'var(--color-text-primary)',
              'border-color': 'var(--color-border-medium)',
              '--tw-ring-color': 'var(--color-focus-ring)',
            }}
          />
        </div>
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
