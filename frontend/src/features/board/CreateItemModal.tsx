import { type Component, createSignal, Show, createEffect, For, createResource, createMemo, createUniqueId } from 'solid-js';
import Modal from '../../shared/ui/Modal';
import RichTextEditor from '../../shared/ui/RichTextEditor';
import Button from '../../shared/ui/Button';
import Field from '../../shared/ui/Field';
import { api } from '../../shared/api';
import type { CreateItem, Item, ItemTypeDef } from '../../shared/types';
import { toast } from '../../shared/ui/toast';
import { resolveLabel } from '../../shared/vocab/vocab';
import { FiPlus, FiX } from 'solid-icons/fi';
import { priorityColor } from '../../shared/ui/PriorityDot';
import { agentProfilesApi, type AgentProfileSummary } from '../../shared/execution';
import ProfilePicker from '../../shared/runWithAgent/ProfilePicker';
import ParentPicker from '../../shared/ui/ParentPicker';
import TypeTile from '../../shared/ui/TypeTile';

/** Choice chip (type / priority / estimate): a pill; the chosen one gets the
 *  accent-soft fill, accent ink and a 2px accent ring. */
const chipClass = (selected: boolean) =>
  'transition-all focus:outline-none focus-visible:ring-2 ' +
  (selected ? 'ring-2 ring-[var(--color-primary-600)]' : 'hover:brightness-[.96]');
const chipStyle = (selected: boolean) => ({
  'background-color': selected ? 'var(--color-accent-soft)' : 'var(--color-bg-subtle)',
  color: selected ? 'var(--color-accent-ink)' : 'var(--color-text-secondary)',
  '--tw-ring-color': 'var(--color-primary-600)',
});
const sectionLabel = 'block text-xs font-semibold mb-2';
const pillInputStyle = {
  'background-color': 'var(--color-bg-base)',
  'border-color': 'var(--color-border-medium)',
  color: 'var(--color-text-primary)',
  '--tw-ring-color': 'var(--color-focus-ring)',
};

export interface CreateItemModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
  projectId: string;
  vocabulary?: Record<string, string>;
  /** The project's type catalog: the choices of Type. */
  itemTypes?: ItemTypeDef[];
  initialStatus?: string;
  mode?: 'create' | 'edit';
  existingItem?: Item;
  parentId?: string;
}

type Priority = 'critical' | 'high' | 'medium' | 'low' | 'none';

const priorityConfig: Record<Priority, { label: string; color: string; emoji: string }> = {
  critical: { label: 'Critical', color: 'red', emoji: '🔥' },
  high: { label: 'High', color: 'orange', emoji: '⬆️' },
  medium: { label: 'Medium', color: 'yellow', emoji: '➡️' },
  low: { label: 'Low', color: 'green', emoji: '⬇️' },
  none: { label: 'None', color: 'gray', emoji: '➖' },
};

const estimateOptions = [0, 0.5, 1, 2, 3, 5, 8, 13, 21];

const CreateItemModal: Component<CreateItemModalProps> = (props) => {
  const mode = () => props.mode || 'create';
  // The catalog's types by what they are for: the ones that hold work, then the ones that are work.
  const holdsWork = createMemo(() => (props.itemTypes ?? []).filter((t) => t.level === 'group'));
  const workTypes = createMemo(() => (props.itemTypes ?? []).filter((t) => t.level !== 'group'));
  const defaultType = () => props.itemTypes?.find((t) => t.default_work)?.key ?? '';
  // Ids that tie each heading to what it names: a <label> to its input, a
  // group's name to its row of option buttons.
  const uid = createUniqueId();
  const ids = { type: `${uid}-type`, priority: `${uid}-priority`, estimate: `${uid}-estimate`, who: `${uid}-who`, parent: `${uid}-parent`, tags: `${uid}-tags` };
  const [title, setTitle] = createSignal('');
  const [description, setDescription] = createSignal('');
  // Empty until a person picks one: the project's default work type, which the server gives an item made without one.
  const [itemType, setItemType] = createSignal('');
  const chosenType = () => itemType() || defaultType();
  const typeName = (key: string) => props.itemTypes?.find((t) => t.key === key)?.name ?? key;
  const [priority, setPriority] = createSignal<Priority>('medium');
  const [estimate, setEstimate] = createSignal<number | null>(null);
  const [tags, setTags] = createSignal<string[]>([]);
  const [tagInput, setTagInput] = createSignal('');
  // Where the item goes: preset by the Add item of a lane or a group, chosen from the parents the server admits.
  const [parentId, setParentId] = createSignal<string | null>(null);
  const [pickingParent, setPickingParent] = createSignal(false);
  const [parentNote, setParentNote] = createSignal('');
  const [profileId, setProfileId] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');

  // The parents the tree rule admits for the chosen type: the preset parent is named from them, and a
  // parent the type cannot go under is dropped rather than refused at Create.
  const [admitted] = createResource(
    () => (props.isOpen && mode() === 'create' && chosenType() ? chosenType() : null),
    (type) => api.items.parents(props.projectId, { type }).catch(() => null),
  );
  const parentChoice = () => admitted()?.find((p) => p.id === parentId());
  createEffect(() => {
    const list = admitted();
    const id = parentId();
    if (!list || !id || list.some((p) => p.id === id)) return;
    setParentId(null);
    setParentNote(`${typeName(chosenType())} cannot go under that parent, so it goes at the top.`);
  });

  // Every profile, to choose who does the new task; none to choose from hides the choice.
  const [profiles] = createResource(
    () => (props.isOpen && mode() === 'create' ? true : null),
    () => agentProfilesApi.list().then((r) => r.data.data ?? [], () => [] as AgentProfileSummary[]),
  );
  const chosenName = () => profiles()?.find((p) => p.agent_profile_id === profileId())?.name ?? 'Nobody yet';

  // Pre-fill form when editing
  createEffect(() => {
    if (props.isOpen && mode() === 'edit' && props.existingItem) {
      const item = props.existingItem;
      setTitle(item.title);
      setDescription(item.description || '');
      setItemType(item.item_type);
      setPriority(item.priority as Priority);
      setEstimate(item.estimate || null);
      setTags(item.tags || []);
    } else if (props.isOpen && mode() === 'create') {
      // Reset form for create mode
      setTitle('');
      setDescription('');
      setItemType('');
      setPriority('medium');
      setEstimate(null);
      setTags([]);
      setParentId(props.parentId ?? null);
      setPickingParent(false);
      setParentNote('');
      setProfileId(null);
    }
  });

  const addTag = () => {
    const tag = tagInput().trim();
    if (tag && !tags().includes(tag)) {
      setTags([...tags(), tag]);
      setTagInput('');
    }
  };

  const removeTag = (tag: string) => {
    setTags(tags().filter((t) => t !== tag));
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    setError('');

    if (!title().trim()) {
      setError('Title is required');
      return;
    }

    setLoading(true);
    try {
      if (mode() === 'edit' && props.existingItem) {
        // Update existing item
        await api.items.update(props.existingItem.id, {
          title: title().trim(),
          description: description().trim() || undefined,
          priority: priority(),
          estimate: estimate() || undefined,
          tags: tags(),
        });
        toast.success('Item updated successfully');
      } else {
        // Create new item
        const itemData: CreateItem = {
          title: title().trim(),
          description: description().trim() || undefined,
          item_type: itemType() || undefined,
          priority: priority(),
          estimate: estimate() || undefined,
          tags: tags(),
          parent_id: parentId(),
        };

        const createdItem = await api.items.create(props.projectId, itemData);

        // Who does it is stored as the profile in the task's run settings.
        const chosen = profileId();
        if (chosen) {
          try {
            await api.items.update(createdItem.id, { run_settings: { agent_profile_id: chosen } });
          } catch {
            toast.error('The item was created, but who does it could not be saved. Choose it again on the item.');
          }
        }

        toast.success('Item created successfully');
      }

      props.onSuccess();
      props.onClose();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : `Failed to ${mode()} item`;
      setError(errorMessage);
      toast.error(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const handleClose = () => {
    if (!loading()) {
      props.onClose();
    }
  };

  return (
    <Modal
      isOpen={props.isOpen}
      onClose={handleClose}
      title={mode() === 'edit' ? 'Edit Item' : 'Create New Item'}
      size="xl"
    >
      <form onSubmit={handleSubmit} class="space-y-5">
        {/* Error message */}
        <Show when={error()}>
          <div
            class="rounded-xl px-4 py-2.5 text-sm font-semibold"
            style={{
              'background-color': 'var(--color-danger-100)',
              color: 'var(--color-danger-600)',
            }}
          >
            {error()}
          </div>
        </Show>

        {/* Title */}
        <Field
          label="Title"
          required
          value={title()}
          onInput={(e) => setTitle(e.currentTarget.value)}
          placeholder="What needs to be done?"
          disabled={loading()}
          autofocus
        />

        {/* Type & Priority Combined Row */}
        <div class="grid grid-cols-2 gap-4">
          {/* Item Type (only in create mode) */}
          <Show when={mode() === 'create'} fallback={<div />}>
            <div>
              <span id={ids.type} class={sectionLabel} style={{ color: "var(--color-text-secondary)" }}>
                Type
              </span>
              <div role="group" aria-labelledby={ids.type} class="space-y-2">
                {/* A catalog with no type that holds work has nothing to tell apart, so it shows no headings. */}
                <For each={[
                  { id: 'holds', heading: 'Holds work', types: holdsWork() },
                  { id: 'work', heading: 'Work', types: workTypes() },
                ].filter((kind) => kind.types.length > 0)}>
                  {(kind) => (
                    <div role="group" aria-label={holdsWork().length > 0 ? kind.heading : undefined} data-testid={`create-types-${kind.id}`}>
                      <Show when={holdsWork().length > 0}>
                        <span class="mb-1 block text-micro font-bold uppercase tracking-wider" style={{ color: 'var(--color-text-tertiary)' }} aria-hidden="true">
                          {kind.heading}
                        </span>
                      </Show>
                      <div class="flex flex-wrap gap-1.5">
                        <For each={kind.types}>
                          {(def) => (
                            <button
                              type="button"
                              onClick={() => setItemType(def.key)}
                              aria-pressed={chosenType() === def.key}
                              class={`px-3 py-1.5 rounded-full text-xs font-semibold ${chipClass(chosenType() === def.key)}`}
                              style={chipStyle(chosenType() === def.key)}
                              disabled={loading()}
                            >
                              {def.name}
                            </button>
                          )}
                        </For>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Priority */}
          <div>
            <span id={ids.priority} class={sectionLabel} style={{ color: "var(--color-text-secondary)" }}>
              Priority
            </span>
            <div role="group" aria-labelledby={ids.priority} class="flex flex-wrap gap-1.5">
              <For each={Object.entries(priorityConfig)}>
                {([prio, config]) => (
                  <button
                    type="button"
                    onClick={() => setPriority(prio as Priority)}
                    aria-pressed={priority() === prio}
                    class={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-semibold ${chipClass(priority() === prio)}`}
                    style={chipStyle(priority() === prio)}
                    disabled={loading()}
                  >
                    <span
                      class="h-2 w-2 shrink-0 rounded-[3px]"
                      style={{ background: priorityColor(prio as Priority) }}
                      aria-hidden="true"
                    />
                    {config.label}
                  </button>
                )}
              </For>
            </div>
          </div>
        </div>

        {/* Parent: always offered in create mode; the picker lists only what the type can go under. */}
        <Show when={mode() === 'create'}>
          <div data-testid="create-parent">
            <span id={ids.parent} class={sectionLabel} style={{ color: 'var(--color-text-secondary)' }}>
              Parent
            </span>
            <div class="flex flex-wrap items-center gap-2">
              <Show
                when={parentId()}
                fallback={<span class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>No parent</span>}
              >
                {(id) => (
                  <span class="inline-flex min-w-0 items-center gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    <Show when={parentChoice()}>{(p) => <TypeTile type={p().type} class="flex-none" />}</Show>
                    <span class="min-w-0 truncate font-semibold" data-parent-id={id()}>{parentChoice()?.title ?? '…'}</span>
                  </span>
                )}
              </Show>
              <Button
                type="button"
                size="sm"
                variant="secondary"
                onClick={() => setPickingParent((open) => !open)}
                aria-expanded={pickingParent()}
                disabled={loading()}
              >
                {pickingParent() ? 'Close' : 'Choose parent'}
              </Button>
            </div>
            <Show when={parentNote()}>
              <p role="status" class="mt-1 text-xs" style={{ color: 'var(--color-text-secondary)' }}>{parentNote()}</p>
            </Show>
            <Show when={pickingParent() && chosenType()}>
              <ParentPicker
                class="mt-2"
                projectId={props.projectId}
                of={{ type: chosenType() }}
                current={parentId()}
                allowNone
                label="Parent"
                onPick={(parent) => {
                  setParentId(parent?.id ?? null);
                  setParentNote('');
                  setPickingParent(false);
                }}
              />
            </Show>
          </div>
        </Show>

        {/* Who does it (only in create mode, and only when there is a profile to choose) */}
        <Show when={mode() === 'create' && (profiles()?.length ?? 0) > 0}>
          <details data-testid="create-who-does-it">
            <summary id={ids.who} class="cursor-pointer text-xs font-semibold" style={{ color: 'var(--color-text-secondary)' }}>
              Who does it: {chosenName()}
            </summary>
            <div class="mt-2">
              <ProfilePicker
                label="Who does it"
                value={profileId()}
                profiles={profiles() ?? []}
                disabled={loading()}
                onChange={(p) => setProfileId(p ? p.agent_profile_id : null)}
              />
            </div>
          </details>
        </Show>

        {/* Description with WYSIWYG */}
        <div>
          <span class={sectionLabel} style={{ color: "var(--color-text-secondary)" }} aria-hidden="true">
            Description
          </span>
          <RichTextEditor
            label="Description"
            value={description()}
            onChange={setDescription}
            placeholder="Add details, acceptance criteria, or notes..."
            disabled={loading()}
          />
        </div>

        {/* Story Points - Compact */}
        <div>
          <span id={ids.estimate} class={sectionLabel} style={{ color: "var(--color-text-secondary)" }}>
            {resolveLabel(props.vocabulary, 'story_points')}
          </span>
          <div role="group" aria-labelledby={ids.estimate} class="flex flex-wrap gap-1.5">
            <For each={estimateOptions}>
              {(value) => (
                <button
                  type="button"
                  onClick={() => setEstimate(estimate() === value ? null : value)}
                  aria-pressed={estimate() === value}
                  class={`w-9 h-9 rounded-full font-mono text-small font-semibold ${chipClass(estimate() === value)}`}
                  style={chipStyle(estimate() === value)}
                  disabled={loading()}
                >
                  {value}
                </button>
              )}
            </For>
          </div>
        </div>

        {/* Tags */}
        <div>
          <label for={ids.tags} class={sectionLabel} style={{ color: "var(--color-text-secondary)" }}>
            Tags
          </label>

          {/* Tag list */}
          <Show when={tags().length > 0}>
            <div class="flex flex-wrap gap-1.5 mb-2">
              <For each={tags()}>
                {(tag) => (
                  <span class="inline-flex items-center gap-1 px-2.5 py-[3px] bg-accent-soft text-accent-ink rounded-full text-micro font-bold">
                    {tag}
                    <button
                      type="button"
                      onClick={() => removeTag(tag)}
                      aria-label={`Remove tag ${tag}`}
                      class="hover:text-danger-500 transition-colors"
                    >
                      <FiX size={12} />
                    </button>
                  </span>
                )}
              </For>
            </div>
          </Show>

          {/* Add tag input */}
          <div class="flex gap-2">
            <input
              id={ids.tags}
              type="text"
              value={tagInput()}
              onInput={(e) => setTagInput(e.currentTarget.value)}
              onKeyPress={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  addTag();
                }
              }}
              placeholder="Add tags (frontend, api...)"
              class="flex-1 min-h-9 px-3.5 py-1.5 border rounded-full text-sm focus:outline-none focus-visible:ring-2 transition-all"
              style={pillInputStyle}
              disabled={loading()}
            />
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={addTag}
              disabled={loading()}
            >
              <FiPlus size={16} />
              Add
            </Button>
          </div>
        </div>

        {/* Actions */}
        <div class="flex gap-3 pt-4" style={{ "border-top": "1px solid var(--color-border-light)" }}>
          <Button type="button" variant="secondary" class="flex-1" onClick={handleClose} disabled={loading()}>
            Cancel
          </Button>
          <Button type="submit" class="flex-1" loading={loading()} disabled={loading() || !title().trim()}>
            {loading()
              ? mode() === 'edit'
                ? 'Updating...'
                : 'Creating...'
              : mode() === 'edit'
                ? 'Update Item'
                : 'Create Item'}
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default CreateItemModal;
