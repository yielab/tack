import { type Component, type JSX, createSignal, createMemo, onMount, onCleanup, Show } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { Button } from '../ui';
import RunWithAgentModal from './RunWithAgentModal';
import { useExecutionStore } from '../state/executionContext';
import { describeExecutionState, type StateTone } from './shared';
import type { RunnerCapabilities } from '../execution';

// Execution chip (Board card): a pill with a leading dot — soft fill, ink
// text. "Running" gets an accent ring and "Waiting on decision", the one
// state that needs a person, is the loudest: a stronger warning fill, ringed.
const CHIP_TONE: Record<StateTone, [bg: string, fg: string, dot: string]> = {
  neutral: ['var(--color-bg-subtle)', 'var(--color-text-secondary)', 'var(--color-text-tertiary)'],
  primary: ['var(--color-accent-soft)', 'var(--color-accent-ink)', 'var(--color-primary-600)'],
  success: ['var(--color-success-100)', 'var(--color-success-600)', 'var(--color-success-600)'],
  warning: ['var(--color-warning-100)', 'var(--color-warning-600)', 'var(--color-warning-600)'],
  danger: ['var(--color-danger-100)', 'var(--color-danger-600)', 'var(--color-danger-600)'],
  info: ['var(--color-info-100)', 'var(--color-info-700)', 'var(--color-info-600)'],
};

function chipStyle(tone: StateTone, raw: string): JSX.CSSProperties {
  const [bg, fg] = CHIP_TONE[tone];
  const base: JSX.CSSProperties = {
    display: 'inline-flex',
    'align-items': 'center',
    gap: '5px',
    'font-size': '10.5px',
    'font-weight': 700,
    padding: '2px 8px',
    'white-space': 'nowrap',
    background: bg,
    color: fg,
  };
  if (raw === 'waiting_decision') {
    return {
      ...base,
      background: 'color-mix(in srgb, var(--color-warning-600) 30%, var(--color-warning-100))',
      color: 'var(--color-text-primary)',
      'box-shadow': '0 0 0 3px var(--color-warning-100)',
    };
  }
  if (raw === 'running') return { ...base, 'box-shadow': '0 0 0 3px var(--color-accent-line)' };
  return base;
}

function chipDot(tone: StateTone, raw: string): string {
  return raw === 'waiting_decision' ? 'var(--color-text-primary)' : CHIP_TONE[tone][2];
}

export interface RunWithAgentButtonProps {
  itemId: string;
  itemTitle: string;
  /** The item's project (`Item.project_id`) — threaded through to the modal
   *  so it can read that project's model default. */
  projectId: string;
  /** Icon-only trigger for tight spaces (Board card header, Sprint lane
   *  row): a single, explicit "run" action, not a menu. `false` renders
   *  a labeled button (item-detail). */
  compact?: boolean;
  /** Shows a small badge, next to the trigger, for the item's most recent
   *  execution request state — registers this item with the shared
   *  store's batched fetch (`store.ts#watchItem`) rather than issuing a
   *  fetch of its own. Off by default; only the Board card mounts it
   *  today. Clicking it opens the item to its Execution tab
   *  (`?item=<id>&tab=execution`), the same tab a successful create
   *  switches item-detail to via `onCreated`. */
  showStateChip?: boolean;
  onCreated?: (requestId: string) => void;
  capabilities?: () => RunnerCapabilities[];
}

/**
 * The one trigger component every "Run with agent" entry point (Board,
 * item-detail, Sprint) mounts to open `RunWithAgentModal` — kept separate
 * from the modal itself so each host only needs one import and one prop set,
 * and so the open/close signal never leaks into a host component's own
 * state: a host gets an entry point and a slot, not a redesign.
 */
const RunWithAgentButton: Component<RunWithAgentButtonProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [, setSearchParams] = useSearchParams();
  const store = useExecutionStore();

  // Registers this item with the store's batched badge fetch for as long as
  // this component is mounted — never a per-card fetch of its own (see
  // `store.ts#watchItem`'s doc comment for how many badges mounting at once
  // still cost exactly one request).
  onMount(() => {
    const unwatch = store.watchItem(props.itemId);
    onCleanup(unwatch);
  });

  // The item's most recent execution request, read from the shared store.
  // `null` whenever there is none yet, or the one record fetched for it
  // errored (an errored record is a real, distinct state, never mistaken
  // for "no activity" — `ExecutionRequestRecord`'s own doc comment). Absence
  // from the cache is conclusive: `watchItem`'s batched fetch asks the
  // server by this exact id, so there is no bounded/truncated preload left
  // to be uncertain about.
  const latestState = createMemo(() => {
    const record = store.requestsForItem(props.itemId)[0];
    if (record?.status === 'ready' && record.summary) {
      return { ...describeExecutionState(record.summary.state), raw: record.summary.state };
    }
    return null;
  });

  const openExecutionTab = (e: MouseEvent) => {
    e.stopPropagation();
    setSearchParams({ item: props.itemId, tab: 'execution' });
  };

  return (
    <>
      <Show when={props.showStateChip && latestState()}>
        {(state) => (
          <button
            type="button"
            aria-label={`Open the Execution tab for ${props.itemTitle}`}
            title="Open the Execution tab"
            onClick={openExecutionTab}
            class="rounded-full focus:outline-none focus-visible:ring-2"
            style={{ ...chipStyle(state().tone, state().raw), border: 'none', cursor: 'pointer' }}
          >
            <span
              aria-hidden="true"
              style={{ width: '6px', height: '6px', 'border-radius': '50%', background: chipDot(state().tone, state().raw), 'flex-shrink': 0 }}
            />
            {state().label}
          </button>
        )}
      </Show>
      {props.compact ? (
        <button
          type="button"
          aria-label={`Run with agent: ${props.itemTitle}`}
          title="Run with agent"
          onClick={(e) => {
            e.stopPropagation();
            setOpen(true);
          }}
          style={{
            width: '20px',
            height: '20px',
            'border-radius': '50%',
            border: 'none',
            background: 'var(--color-bg-panel)',
            cursor: 'pointer',
            color: 'var(--color-text-secondary)',
            display: 'grid',
            'place-items': 'center',
            'line-height': '1',
            'font-size': '9px',
            padding: 0,
            'flex-shrink': 0,
          }}
          class="focus:outline-none focus-visible:ring-2"
        >
          ▶
        </button>
      ) : (
        <Button
          size="sm"
          variant="primary"
          onClick={(e) => {
            e.stopPropagation();
            setOpen(true);
          }}
        >
          Run with agent
        </Button>
      )}
      <RunWithAgentModal
        isOpen={open()}
        onClose={() => setOpen(false)}
        itemId={props.itemId}
        itemTitle={props.itemTitle}
        projectId={props.projectId}
        onCreated={props.onCreated}
        capabilities={props.capabilities}
      />
    </>
  );
};

export default RunWithAgentButton;
