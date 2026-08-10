import { type Component, createSignal } from 'solid-js';
import { Button } from '../ui';
import RunWithAgentModal from './RunWithAgentModal';
import type { RunnerCapabilities } from '../execution';

export interface RunWithAgentButtonProps {
  itemId: string;
  itemTitle: string;
  /** Icon-only trigger for tight spaces (Board card header, Sprint lane
   *  row) — visually and structurally distinct from `shared/dispatch/
   *  DispatchCardMenu.tsx`'s "⋮" kebab menu, per III.0's vocabulary rule:
   *  this is a single, explicit "run" action, not a menu that happens to
   *  contain one. `false` renders a labeled button (item-detail). */
  compact?: boolean;
  onCreated?: (requestId: string) => void;
  capabilities?: () => RunnerCapabilities[];
}

/**
 * The one trigger component every "Run with agent" entry point (Board,
 * item-detail, Sprint) mounts to open `RunWithAgentModal` — kept separate
 * from the modal itself so each host only needs one import and one prop set,
 * and so the open/close signal never leaks into a host component's own
 * state (TODO.md III-E4's "bounded edits" instruction: hosts get an entry
 * point and a slot, not a redesign).
 */
const RunWithAgentButton: Component<RunWithAgentButtonProps> = (props) => {
  const [open, setOpen] = createSignal(false);

  return (
    <>
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
            width: '18px',
            height: '18px',
            'border-radius': '4px',
            border: 'none',
            background: 'transparent',
            cursor: 'pointer',
            color: 'var(--color-text-tertiary)',
            'line-height': '1',
            'font-size': '12px',
            padding: 0,
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
        onCreated={props.onCreated}
        capabilities={props.capabilities}
      />
    </>
  );
};

export default RunWithAgentButton;
