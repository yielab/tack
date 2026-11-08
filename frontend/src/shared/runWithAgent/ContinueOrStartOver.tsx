import { type Component, Show, createSignal } from 'solid-js';
import { Button } from '../ui';
import { toast } from '../ui/toast';
import { executionsApi, readContinueOffer, readKeptWork, type AttemptSummary } from '../execution';
import RunWithAgentModal from './RunWithAgentModal';
import { doubled, stoppedByLimit } from './runLimit';

export interface ContinueOrStartOverProps {
  attempt: AttemptSummary;
  /** The run's own token limit (`ExecutionSummary.limit.tokens`), to double when "Start over from
   *  this run's work" starts from a stop whose own limit really was tokens. */
  tokenLimit?: number | null;
  /** With the task's title and project, "Start over from this run's work" can open the dialog. */
  itemId?: string;
  itemTitle?: string;
  projectId?: string;
  onCreated?: (requestId: string) => void;
}

/**
 * What a run a step's own budget stopped offers next: the backend's own sentence in red, where its kept work sits
 * when it kept any, and the two new actions — **Continue** the same docket task, or **Start over
 * from this run's work** — each always on screen and disabled with its reason when the case does
 * not offer it (ADR 0032: a control this screen cannot do is dimmed, never hidden). "Start over,
 * clean" is the plain "Run again" already offered elsewhere on this screen; this block does not
 * repeat it.
 *
 * Renders nothing when the attempt's ending never carried `terminal_reason.next.continue` at
 * all — every failure that is not a step's own budget stop, including one from a docket too old
 * to say so.
 */
const ContinueOrStartOver: Component<ContinueOrStartOverProps> = (props) => {
  const offer = () => readContinueOffer(props.attempt.terminal_reason);
  const kept = () => readKeptWork(props.attempt.terminal_reason);
  // The sentence docket.rs already composed ("The step ran out of its round limit: 20 of 20.
  // Raise maxIterations to allow more."); shown exactly as the backend wrote it.
  const message = () => {
    const reason = props.attempt.terminal_reason as { message?: unknown } | null;
    return typeof reason?.message === 'string' && reason.message !== '' ? reason.message : null;
  };

  const [open, setOpen] = createSignal(false);
  const [continuing, setContinuing] = createSignal(false);
  const knowsTask = () => !!props.itemTitle && !!props.projectId;

  // One of the two new actions is primed when it is the thing to do; the clean "Run again"
  // elsewhere on the screen stays the default when neither applies (a step that changed nothing).
  const primary = (): 'continue' | 'start-over-work' | null => {
    if (offer()?.possible) return 'continue';
    if (kept()) return 'start-over-work';
    return null;
  };

  const continueReason = (): string | undefined => {
    const o = offer();
    return o && !o.possible ? o.why ?? 'docket cannot continue this step.' : undefined;
  };
  const startOverWorkReason = (): string | undefined => {
    if (!kept()) return "This run's step changed nothing, so there is no work to start from.";
    return knowsTask() ? undefined : 'This screen does not know the task';
  };

  const continueRun = async () => {
    setContinuing(true);
    try {
      const result = await executionsApi.continue(props.attempt.request_id);
      toast.success(result.replayed ? 'Reused the run already continuing this step.' : 'Continuing the step.');
      props.onCreated?.(result.request_id);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to continue the step.');
    } finally {
      setContinuing(false);
    }
  };

  return (
    <Show when={offer()}>
      <div data-testid="continue-or-start-over" class="space-y-2">
        <Show when={message()}>
          {(text) => (
            <p data-testid="budget-stop-message" class="text-sm" style={{ color: 'var(--color-danger-600)' }}>
              {text()}
            </p>
          )}
        </Show>
        <Show when={kept()}>
          {(k) => (
            <p data-testid="budget-stop-kept-work" class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              Its work is kept on <code style={{ 'font-family': 'var(--font-mono)' }}>{k().branch}</code> — see Changes, below, for the diff.
            </p>
          )}
        </Show>
        <div class="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            variant={primary() === 'continue' ? 'primary' : 'secondary'}
            disabled={!offer()?.possible || continuing()}
            loading={continuing()}
            reason={continueReason()}
            onClick={() => void continueRun()}
          >
            Continue
          </Button>
          <Button
            size="sm"
            variant={primary() === 'start-over-work' ? 'primary' : 'secondary'}
            disabled={!kept() || !knowsTask()}
            reason={startOverWorkReason()}
            onClick={() => setOpen(true)}
          >
            Start over from this run's work
          </Button>
        </div>
        <Show when={kept() && knowsTask()}>
          <RunWithAgentModal
            isOpen={open()}
            onClose={() => setOpen(false)}
            itemId={props.itemId!}
            itemTitle={props.itemTitle!}
            projectId={props.projectId!}
            fromRun={props.attempt.request_id}
            initialTokenLimit={
              stoppedByLimit(props.attempt.terminal_reason) && props.tokenLimit ? doubled(props.tokenLimit) : undefined
            }
            onCreated={props.onCreated}
          />
        </Show>
      </div>
    </Show>
  );
};

export default ContinueOrStartOver;
