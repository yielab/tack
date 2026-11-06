import { type Component, type JSX, Show, createSignal, createUniqueId } from 'solid-js';
import { A } from '@solidjs/router';
import clsx from 'clsx';
import RunWithAgentButton from '../runWithAgent/RunWithAgentButton';
import type { Item, WorkflowStatus } from '../types';
import Button from './Button';
import { readFoldOpen, saveFoldOpen } from './Fold';
import { IconChevronDown, IconPlus } from './icons';
import ItemMenu from './ItemMenu';
import ItemPath, { itemHref } from './ItemPath';
import ProgressBar from './ProgressBar';
import { statusDotColor } from './statusDot';
import TypeTile from './TypeTile';

export interface GroupHeaderProps {
  /** The group, as the server read it: its type, status and `progress` are drawn as given. */
  group: Item;
  /** Every item the view holds, by id, for the path above the title. */
  byId: ReadonlyMap<string, Item>;
  /** `lane` is the compact row over a Board lane; `heading` is the larger one above a group's items in a list. */
  density: 'lane' | 'heading';
  /** The project's workflow statuses: the chip's menu offers them. */
  statuses: WorkflowStatus[];
  /** Sets the group's status. Without it the chip only shows the status. */
  onStatusChange?: (status: string) => void;
  /** The project's word for points, when it has its own. */
  pointsWord?: string;
  /** Adds an item under the group. */
  onAddItem?: () => void;
  /** Opens the picker that moves the group under another one: the keyboard's way to do what dragging does. */
  onMoveUnder?: () => void;
  /** Shut or open when the person never chose; open when absent. The choice is remembered per group, in this browser. */
  defaultOpen?: boolean;
  /** Told each time the person folds or unfolds the group, with the new state. */
  onToggle?: (open: boolean) => void;
  /** A sentence the server gave for turning down something done to this group or under it; it stays until the owner clears it. */
  refusal?: string | null;
  class?: string;
  style?: JSX.CSSProperties;
  /** Extra style for the header row alone (a lane keeps it in sight while the work beside it scrolls sideways). */
  headerStyle?: JSX.CSSProperties;
  /** What lies under the group; it folds with the header. */
  children?: JSX.Element;
}

const StatusDot: Component<{ category: string | undefined }> = (props) => (
  <span aria-hidden="true" class="flex-none" style={{ width: '8px', height: '8px', 'border-radius': '50%', background: statusDotColor(props.category) }} />
);

/**
 * The one way a group is drawn above its work: its path, its type's tile, its title as a link to its
 * page, a status chip that opens the status menu, how far its work is, the teams that have it, a fold,
 * and the two things to do with an empty or growing group: Add item and Plan with agent. Every
 * control stays visible. The fold is the header's own button (a link cannot sit inside `Fold`'s), and it
 * is remembered per group the way `Fold` remembers a section.
 */
const GroupHeader: Component<GroupHeaderProps> = (props) => {
  const panelId = createUniqueId();
  const foldKind = () => `group:${props.group.id}`;
  const [open, setOpen] = createSignal(readFoldOpen(foldKind()) ?? props.defaultOpen ?? true);
  const toggle = () => {
    const next = !open();
    setOpen(next);
    saveFoldOpen(foldKind(), next);
    props.onToggle?.(next);
  };
  const lane = () => props.density === 'lane';
  const category = (status: string) => props.statuses.find((s) => s.name === status)?.category;
  const teams = () => props.group.progress?.teams ?? [];
  const chip = (
    <>
      <StatusDot category={category(props.group.status)} />
      <span>{props.group.status}</span>
    </>
  );

  return (
    <section
      data-testid="group-header"
      data-density={props.density}
      data-group-id={props.group.id}
      aria-label={props.group.title}
      class={clsx('min-w-0', props.class)}
      style={props.style}
    >
      <div
        class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5"
        style={{ padding: lane() ? '8px 12px' : '8px 0', ...props.headerStyle }}
      >
        <button
          type="button"
          aria-expanded={open()}
          aria-controls={panelId}
          aria-label={`Work under ${props.group.title}`}
          onClick={toggle}
          class="grid h-7 w-7 flex-none place-items-center rounded-full focus:outline-none focus-visible:ring-2"
          style={{ color: 'var(--color-text-secondary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
        >
          <span aria-hidden="true" class="inline-flex" style={{ transform: open() ? 'rotate(0deg)' : 'rotate(-90deg)' }}>
            <IconChevronDown size={14} />
          </span>
        </button>
        <div class="flex min-w-0 flex-col gap-0.5">
          <ItemPath item={props.group} byId={props.byId} projectId={props.group.project_id} />
          <div class="flex min-w-0 items-center gap-2">
            <TypeTile type={props.group.item_type} class="flex-none" />
            <A
              href={itemHref(props.group.project_id, props.group.id)}
              title={props.group.title}
              class="min-w-0 truncate font-bold hover:underline focus:outline-none focus-visible:ring-2"
              style={{
                'font-family': 'var(--font-body)',
                'font-size': lane() ? '0.9375rem' : '1.25rem',
                color: 'var(--color-text-primary)',
                '--tw-ring-color': 'var(--color-focus-ring)',
              }}
            >
              {props.group.title}
            </A>
          </div>
        </div>
        <Show
          when={props.onStatusChange}
          fallback={
            <span
              data-testid="group-status"
              class="inline-flex h-7 items-center gap-1.5 rounded-full px-3 text-xs font-semibold"
              style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)' }}
            >
              {chip}
            </span>
          }
        >
          {(change) => (
            <span data-testid="group-status" class="inline-flex">
              <ItemMenu
                align="left"
                label={chip}
                icon={<IconChevronDown size={12} />}
                buttonLabel={`Status of ${props.group.title}: ${props.group.status}. Change status`}
                menuLabel="Status"
                entries={props.statuses.map((s) => ({
                  label: s.name,
                  current: s.name === props.group.status,
                  leading: <StatusDot category={s.category} />,
                  onSelect: () => { if (s.name !== props.group.status) change()(s.name); },
                }))}
              />
            </span>
          )}
        </Show>
        <ProgressBar progress={props.group.progress} pointsWord={props.pointsWord} compact={lane()} />
        <Show when={teams().length > 0}>
          <span data-testid="group-teams" title="Teams" class="min-w-0 truncate text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            {teams().join(', ')}
          </span>
        </Show>
        <div class="ml-auto flex flex-none flex-wrap items-center gap-2">
          <Show when={props.onMoveUnder}>
            {(move) => (
              <Button size="sm" variant="ghost" onClick={move()} aria-label={`Move under…, ${props.group.title}`}>
                Move under…
              </Button>
            )}
          </Show>
          <Show when={props.onAddItem}>
            {(add) => (
              <Button size="sm" variant="secondary" onClick={add()} aria-label={`Add item to ${props.group.title}`}>
                <IconPlus size={13} />
                Add item
              </Button>
            )}
          </Show>
          <RunWithAgentButton
            itemId={props.group.id}
            itemTitle={props.group.title}
            projectId={props.group.project_id}
            variant="secondary"
            label="Plan with agent"
          />
        </div>
      </div>
      <Show when={props.refusal}>
        {(why) => (
          <p
            role="alert"
            data-testid="group-refusal"
            class="m-0 px-3 pb-2 text-xs font-semibold"
            style={{ color: 'var(--color-danger-600)' }}
          >
            {why()}
          </p>
        )}
      </Show>
      <div id={panelId} hidden={!open()}>
        {props.children}
      </div>
    </section>
  );
};

export default GroupHeader;
