import { type Component, createEffect, createMemo, createResource, createSignal, createUniqueId, For, Show } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, Modal, TypeTile } from '../../../shared/ui';
import { useProject } from '../../../shared/state/projectContext';
import type { ItemTypeUse, TypeColor, TypeLevel } from '../../../shared/types';

const COLORS: Array<{ value: TypeColor; label: string }> = [
  { value: 'accent', label: 'Accent' },
  { value: 'accent2', label: 'Second accent' },
  { value: 'success', label: 'Success' },
  { value: 'warning', label: 'Warning' },
  { value: 'danger', label: 'Danger' },
  { value: 'neutral', label: 'Neutral' },
];

const LEVELS: Array<{ value: TypeLevel; label: string }> = [
  { value: 'group', label: 'Holds work' },
  { value: 'work', label: 'Work' },
];

const fieldClass = 'min-h-8 rounded-full border px-3 py-1 text-small focus:outline-none focus-visible:ring-2 disabled:cursor-not-allowed disabled:opacity-60';
const fieldStyle = {
  'background-color': 'var(--color-bg-base)',
  color: 'var(--color-text-primary)',
  'border-color': 'var(--color-border-medium)',
  '--tw-ring-color': 'var(--color-focus-ring)',
} as const;
const MUTED = { color: 'var(--color-text-secondary)' } as const;

const itemsText = (n: number) => `${n} ${n === 1 ? 'item' : 'items'}`;

/** A key for a new type's name: lowercase letters, digits and dashes, as the project file keeps it. */
export function keyFor(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 40)
    .replace(/-+$/, '');
}

/** Why a type's kind is fixed: the reason is the count, since items of the other kind could not sit where these do. */
const kindReason = (use: ItemTypeUse) => `Fixed while ${itemsText(use.items)} use${use.items === 1 ? 's' : ''} it. Give them another type first.`;

/**
 * Settings → Types: the project's catalog. Each type is a row (name, whether it holds work or is
 * work, how many items use it, its colour, whether it is the one new work takes, its place), a
 * new type is added below, and a type is removed by moving its items to another one first.
 */
const TypesPanel: Component = () => {
  const { projectId, refetch: refetchProject } = useProject();
  const [types, { refetch }] = createResource(
    () => projectId(),
    (id) => api.itemTypes.list(id),
  );
  // Rows keep their place in the page (and the focus inside them) when the catalog is read again.
  const [rows, setRows] = createStore<ItemTypeUse[]>([]);
  createEffect(() => setRows(reconcile(types() ?? [], { key: 'key' })));
  const list = () => rows;
  const [announce, setAnnounce] = createSignal('');
  const [removing, setRemoving] = createSignal<ItemTypeUse | null>(null);

  const reload = async () => {
    await Promise.all([refetch(), Promise.resolve(refetchProject())]);
  };

  /** One change to a type, then the catalog is read again so every row shows what the server holds. */
  const change = async (work: () => Promise<unknown>, failure: string) => {
    try {
      await work();
      await reload();
      return true;
    } catch (err) {
      toast.error(err instanceof Error ? err.message : failure);
      await reload();
      return false;
    }
  };

  const update = (key: string, patch: Parameters<typeof api.itemTypes.update>[2]) =>
    change(() => api.itemTypes.update(projectId()!, key, patch), 'Could not save the type');

  const moveTo = async (key: string, to: number) => {
    const at = list().findIndex((t) => t.key === key);
    const target = Math.max(0, Math.min(list().length - 1, to));
    if (at < 0 || target === at) return;
    const name = list()[at].name;
    const handleHadFocus = document.activeElement?.closest('[data-testid="type-handle"]') !== null;
    if (await update(key, { order: target })) setAnnounce(`${name} is now number ${target + 1} of ${list().length}`);
    if (handleHadFocus) document.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"] [data-testid="type-handle"]`)?.focus();
  };

  return (
    <div class="grid max-w-[920px] gap-[18px]">
      <p class="text-small" style={MUTED}>
        What an item can be. A type that holds work (like an epic) is planned, shows how far its work is and is never
        run itself; work is what gets done and run. New work takes the default type.
      </p>

      <Show when={types.error}>
        <p role="alert" class="text-small" style={{ color: 'var(--color-danger-600)' }}>
          The types did not load. Reload the page to try again.
        </p>
      </Show>

      <ul aria-label="Types" class="m-0 flex list-none flex-col gap-2 p-0" data-testid="types-list">
        <For each={list()}>
          {(use, index) => (
            <TypeRow
              use={use}
              index={index()}
              count={list().length}
              onUpdate={(patch) => update(use.key, patch)}
              groupName={`default-work-${projectId()}`}
              onMove={(to) => moveTo(use.key, to)}
              onDropFrom={(from) => moveTo(from, index())}
              onRemove={() => setRemoving(use)}
            />
          )}
        </For>
      </ul>
      <p class="sr-only" role="status" data-testid="types-announce">
        {announce()}
      </p>

      <AddType types={list()} onAdd={(input) => change(() => api.itemTypes.create(projectId()!, input), 'Could not add the type')} />

      <Show when={removing()}>
        {(use) => (
          <RemoveType
            use={use()}
            others={list().filter((t) => t.key !== use().key && t.level === use().level)}
            onClose={() => setRemoving(null)}
            onRemove={async (mergeInto) => {
              const ok = await change(() => api.itemTypes.remove(projectId()!, use().key, mergeInto), 'Could not remove the type');
              if (ok) setRemoving(null);
            }}
          />
        )}
      </Show>
    </div>
  );
};

const TypeRow: Component<{
  use: ItemTypeUse;
  index: number;
  count: number;
  groupName: string;
  onUpdate: (patch: Parameters<typeof api.itemTypes.update>[2]) => Promise<boolean>;
  onMove: (to: number) => void;
  /** Another row was dropped on this one: it takes this row's place. */
  onDropFrom: (key: string) => void;
  onRemove: () => void;
}> = (props) => {
  const u = () => props.use;
  const ids = createUniqueId();
  let row: HTMLLIElement | undefined;
  const isGroup = () => u().level === 'group';
  const locked = () => u().items > 0;
  const defaultReason = () => (isGroup() ? `${u().name} holds work, so new work cannot take it as its type.` : undefined);
  const removeReason = () => (u().default_work ? `${u().name} is the default type. Choose another default first.` : undefined);
  const [name, setName] = createSignal(u().name);
  createEffect(() => setName(u().name));
  /** Saves a change; when the server refuses it the control goes back to what the server holds. */
  const save = async (patch: Parameters<typeof props.onUpdate>[0], undo: () => void) => {
    if (!(await props.onUpdate(patch))) undo();
  };

  const onHandleKey = (e: KeyboardEvent) => {
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      props.onMove(props.index - 1);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      props.onMove(props.index + 1);
    } else if (e.key === 'Home') {
      e.preventDefault();
      props.onMove(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      props.onMove(props.count - 1);
    }
  };

  return (
    <li
      ref={row}
      data-testid="type-row"
      data-key={u().key}
      class="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-item px-3 py-2"
      style={{ 'background-color': 'var(--color-bg-panel)' }}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        const from = e.dataTransfer?.getData('text/plain');
        if (from && from !== u().key) props.onDropFrom(from);
      }}
    >
      <button
        type="button"
        data-testid="type-handle"
        aria-label={`Move ${u().name}`}
        aria-describedby={`${ids}-handle`}
        draggable={true}
        onDragStart={(e) => {
          e.dataTransfer?.setData('text/plain', u().key);
          if (e.dataTransfer) {
            e.dataTransfer.effectAllowed = 'move';
            if (row) e.dataTransfer.setDragImage?.(row, 0, 0);
          }
        }}
        onKeyDown={onHandleKey}
        class="grid h-8 w-6 flex-none cursor-grab place-items-center rounded-item focus:outline-none focus-visible:ring-2"
        style={{ background: 'none', border: 0, color: 'var(--color-text-tertiary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
      >
        <svg width="10" height="16" viewBox="0 0 10 16" fill="currentColor" aria-hidden="true">
          <circle cx="2.5" cy="3" r="1.4" />
          <circle cx="7.5" cy="3" r="1.4" />
          <circle cx="2.5" cy="8" r="1.4" />
          <circle cx="7.5" cy="8" r="1.4" />
          <circle cx="2.5" cy="13" r="1.4" />
          <circle cx="7.5" cy="13" r="1.4" />
        </svg>
      </button>
      <span id={`${ids}-handle`} class="sr-only">
        Press the up or down arrow to move it. It is number {props.index + 1} of {props.count}.
      </span>

      <div class="flex min-w-[10rem] flex-1 items-center gap-2">
        <TypeTile type={u().key} label={u().name} class="flex-none" />
        <input
          type="text"
          class={`${fieldClass} min-w-0 flex-1`}
          style={fieldStyle}
          aria-label={`Name of the type ${u().name}`}
          value={name()}
          maxLength={60}
          onInput={(e) => setName(e.currentTarget.value)}
          onChange={(e) => {
            const next = e.currentTarget.value.trim();
            if (!next) setName(u().name);
            else if (next !== u().name) void save({ name: next }, () => setName(u().name));
          }}
        />
      </div>

      <div class="flex flex-col gap-0.5">
        <select
          class={fieldClass}
          style={fieldStyle}
          aria-label={`${u().name} holds work or is work`}
          aria-describedby={locked() ? `${ids}-kind` : undefined}
          disabled={locked()}
          onChange={(e) => {
            const select = e.currentTarget;
            void save({ level: select.value as TypeLevel }, () => (select.value = u().level));
          }}
        >
          <For each={LEVELS}>{(l) => <option value={l.value} selected={u().level === l.value}>{l.label}</option>}</For>
        </select>
        <Show when={locked()}>
          <p id={`${ids}-kind`} data-testid="type-kind-reason" class="max-w-[16rem] text-caption" style={MUTED}>
            {kindReason(u())}
          </p>
        </Show>
      </div>

      <span data-testid="type-items" class="w-16 flex-none text-small tabular-nums" style={MUTED}>
        {itemsText(u().items)}
      </span>

      <select
        class={fieldClass}
        style={fieldStyle}
        aria-label={`Colour of ${u().name}`}
        onChange={(e) => {
          const select = e.currentTarget;
          void save({ color: select.value as TypeColor }, () => (select.value = u().color));
        }}
      >
        <For each={COLORS}>{(c) => <option value={c.value} selected={u().color === c.value}>{c.label}</option>}</For>
      </select>

      <label class="flex items-center gap-1.5 text-small" style={{ color: 'var(--color-text-primary)' }} title={defaultReason()}>
        <input
          type="radio"
          name={props.groupName}
          data-testid="type-default"
          checked={u().default_work}
          disabled={isGroup()}
          aria-describedby={defaultReason() ? `${ids}-default` : undefined}
          onChange={(e) => {
            const radio = e.currentTarget;
            void save({ default_work: true }, () => (radio.checked = u().default_work));
          }}
        />
        Default
        <Show when={defaultReason()}>
          <span id={`${ids}-default`} class="sr-only">
            {defaultReason()}
          </span>
        </Show>
      </label>

      <Button size="sm" variant="secondary" disabled={u().default_work} reason={removeReason()} aria-label={`Remove ${u().name}`} onClick={props.onRemove}>
        Remove
      </Button>
    </li>
  );
};

const AddType: Component<{
  types: ItemTypeUse[];
  onAdd: (input: { key: string; name: string; level: TypeLevel; color: TypeColor }) => Promise<boolean>;
}> = (props) => {
  const [name, setName] = createSignal('');
  const [level, setLevel] = createSignal<TypeLevel>('work');
  const [color, setColor] = createSignal<TypeColor>('neutral');
  const [busy, setBusy] = createSignal(false);
  const key = createMemo(() => keyFor(name()));
  const reason = createMemo(() => {
    if (!name().trim()) return 'Give it a name.';
    if (!key()) return 'Its name needs a letter or a digit.';
    if (props.types.some((t) => t.key === key())) return 'A type with this key already exists. Choose another name.';
    return undefined;
  });

  const submit = async (e: Event) => {
    e.preventDefault();
    if (reason() || busy()) return;
    setBusy(true);
    try {
      if (await props.onAdd({ key: key(), name: name().trim(), level: level(), color: color() })) {
        setName('');
        setLevel('work');
        setColor('neutral');
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} class="flex flex-col gap-2" aria-label="Add a type" data-testid="add-type">
      <div class="flex flex-wrap items-end gap-3">
        <label class="flex min-w-[10rem] flex-1 flex-col gap-1 text-small font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Name
          <input
            type="text"
            class={`${fieldClass} font-normal`}
            style={fieldStyle}
            value={name()}
            maxLength={60}
            placeholder="Building, Course, Bug…"
            onInput={(e) => setName(e.currentTarget.value)}
          />
        </label>
        <label class="flex flex-col gap-1 text-small font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Holds work or is work
          <select class={`${fieldClass} font-normal`} style={fieldStyle} value={level()} onChange={(e) => setLevel(e.currentTarget.value as TypeLevel)}>
            <For each={LEVELS}>{(l) => <option value={l.value}>{l.label}</option>}</For>
          </select>
        </label>
        <label class="flex flex-col gap-1 text-small font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Colour
          <select class={`${fieldClass} font-normal`} style={fieldStyle} value={color()} onChange={(e) => setColor(e.currentTarget.value as TypeColor)}>
            <For each={COLORS}>{(c) => <option value={c.value}>{c.label}</option>}</For>
          </select>
        </label>
        <Button type="submit" loading={busy()} disabled={!!reason() || busy()} reason={name().trim() ? reason() : undefined}>
          Add type
        </Button>
      </div>
      <Show when={key()}>
        <p class="text-caption" style={MUTED}>
          Its key in project files: <code>{key()}</code>. A key never changes once the type exists.
        </p>
      </Show>
    </form>
  );
};

/** The question before a type goes: where its items move to. Only a type of the same kind can take them. */
const RemoveType: Component<{
  use: ItemTypeUse;
  others: ItemTypeUse[];
  onClose: () => void;
  onRemove: (mergeInto: string | undefined) => Promise<void>;
}> = (props) => {
  const ids = createUniqueId();
  const first = () => props.others.find((t) => t.default_work)?.key ?? props.others[0]?.key ?? '';
  const [target, setTarget] = createSignal(first());
  const [busy, setBusy] = createSignal(false);
  const n = () => props.use.items;
  const stuck = () => n() > 0 && props.others.length === 0;

  return (
    <Modal isOpen onClose={props.onClose} title={`Remove ${props.use.name}`} size="sm">
      <form
        class="flex flex-col gap-4"
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          try {
            await props.onRemove(n() > 0 ? target() : undefined);
          } finally {
            setBusy(false);
          }
        }}
      >
        <Show
          when={n() > 0}
          fallback={
            <p class="text-sm" style={MUTED}>
              No item uses {props.use.name}. It is removed from the project.
            </p>
          }
        >
          <label for={`${ids}-into`} class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Move its {n()} {n() === 1 ? 'item' : 'items'} to…
          </label>
          <select
            id={`${ids}-into`}
            class={fieldClass}
            style={fieldStyle}
            value={target()}
            disabled={stuck()}
            aria-describedby={stuck() ? `${ids}-stuck` : undefined}
            onChange={(e) => setTarget(e.currentTarget.value)}
          >
            <For each={props.others}>{(t) => <option value={t.key} selected={t.key === target()}>{t.name}</option>}</For>
          </select>
          <Show when={stuck()}>
            <p id={`${ids}-stuck`} class="text-caption" style={MUTED}>
              No other type {props.use.level === 'group' ? 'holds work' : 'is work'}, so they have nowhere to go. Add one first.
            </p>
          </Show>
          <p class="text-caption" style={MUTED}>
            They keep their place in the tree, and only their type changes.
          </p>
        </Show>
        <div class="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={props.onClose}>
            Cancel
          </Button>
          <Button type="submit" variant="danger" loading={busy()} disabled={busy() || stuck()}>
            {n() > 0 ? 'Move and remove' : 'Remove'}
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default TypesPanel;
