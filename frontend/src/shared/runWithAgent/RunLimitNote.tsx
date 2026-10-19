import { type Component, Show, createSignal } from 'solid-js';
import { Button } from '../ui';
import type { RunLimit } from '../execution/api';
import RunWithAgentModal from './RunWithAgentModal';
import { doubled, isNearLimit, spaced } from './runLimit';

export interface RunLimitNoteProps {
  limit: RunLimit;
  /** The run is still going: its token count shows, amber near the limit. */
  running: boolean;
  /** The run's token limit stopped it. */
  stopped: boolean;
  /** The test id of the limit sentence, which each screen keeps. */
  testId: string;
  /** With the task's title and project, a stopped run offers "Run again with more". */
  itemId?: string;
  itemTitle?: string;
  projectId?: string;
  onCreated?: (requestId: string) => void;
}

/** The run's limit sentence, its live token count while it runs, and, once its limit stopped
 *  it, how much it used and a way to run it again with twice the limit. */
const RunLimitNote: Component<RunLimitNoteProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const tokens = () => props.limit.tokens;
  const used = () => props.limit.used ?? null;
  const near = () => isNearLimit(used(), tokens());
  return (
    <>
      <p data-testid={props.testId} class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
        {props.limit.sentence}
      </p>
      <Show when={props.running && tokens() !== null && used() !== null}>
        <p
          data-testid="run-limit-used"
          data-near-limit={near()}
          class="text-xs"
          style={{ color: near() ? 'var(--color-warning-600)' : 'var(--color-text-secondary)' }}
        >
          Used {spaced(used()!)} of {spaced(tokens()!)} tokens
        </p>
      </Show>
      <Show when={props.stopped && tokens() !== null}>
        <div class="flex flex-wrap items-center gap-2">
          <p data-testid="run-limit-stopped" class="text-sm" style={{ color: 'var(--color-danger-600)' }}>
            Stopped at its limit: {spaced(used() ?? tokens()!)} of {spaced(tokens()!)}
          </p>
          <Show when={props.itemId && props.itemTitle && props.projectId}>
            <Button size="sm" variant="secondary" onClick={() => setOpen(true)}>
              Run again with more
            </Button>
            <RunWithAgentModal
              isOpen={open()}
              onClose={() => setOpen(false)}
              itemId={props.itemId!}
              itemTitle={props.itemTitle!}
              projectId={props.projectId!}
              initialTokenLimit={doubled(tokens()!)}
              onCreated={props.onCreated}
            />
          </Show>
        </div>
      </Show>
    </>
  );
};

export default RunLimitNote;
