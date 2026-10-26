import { type Component, Show, createSignal } from 'solid-js';
import { Button } from '../ui';
import type { AttemptSummary } from '../execution';
import RunWithAgentModal from './RunWithAgentModal';

export interface RerunAtL3Props {
  attempt: AttemptSummary;
  /** With the task's title and project the button can start a run; without them it says so. */
  itemId: string;
  itemTitle?: string;
  projectId?: string;
  onCreated?: (requestId: string) => void;
}

/** Whether an attempt ended because the agent needed a person. */
export const needsAPerson = (attempt: AttemptSummary | undefined): boolean =>
  attempt?.state === 'failed' && attempt.terminal_reason?.code === 'needs_human';

/** What an attempt that ended asking for a person offers: a new run of the same task that is
 *  attended from the start (L3). Disabled, with the board's reason, when the team cannot be
 *  narrowed or the project is already at L3; it never starts a run by itself. */
const RerunAtL3: Component<RerunAtL3Props> = (props) => {
  const [open, setOpen] = createSignal(false);
  const knowsTask = () => !!props.itemTitle && !!props.projectId;
  const enabled = () => knowsTask() && props.attempt.rerun_at_l3?.enabled === true;
  const reason = (): string | null => {
    if (!knowsTask()) return 'This screen does not know the task';
    const state = props.attempt.rerun_at_l3;
    if (!state) return 'The board did not say whether this run can be narrowed';
    return state.enabled ? null : (state.reason ?? 'It cannot be narrowed');
  };
  return (
    <div class="flex flex-wrap items-center gap-2">
      <Button size="sm" variant="secondary" disabled={!enabled()} title={reason() ?? undefined} onClick={() => setOpen(true)}>
        Run again at L3
      </Button>
      <Show when={reason()}>
        {(why) => (
          <span data-testid="rerun-l3-reason" class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            {why()}
          </span>
        )}
      </Show>
      <Show when={enabled()}>
        <RunWithAgentModal
          isOpen={open()}
          onClose={() => setOpen(false)}
          itemId={props.itemId}
          itemTitle={props.itemTitle!}
          projectId={props.projectId!}
          autonomy="L3"
          onCreated={props.onCreated}
        />
      </Show>
    </div>
  );
};

export default RerunAtL3;
