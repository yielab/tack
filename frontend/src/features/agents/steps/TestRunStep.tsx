import { type Component, Show, createMemo, createSignal } from 'solid-js';
import { toast } from '../../../shared/ui/toast';
import { Button } from '../../../shared/ui';
import type { RunnerSummary } from '../../../shared/execution/api';
import { useExecutionStore } from '../../../shared/state/executionContext';
import ExecutionTimeline from '../../../shared/runWithAgent/ExecutionTimeline';
import { findHarness, harnessesOf, isReady } from '../runnerObservations';
import { HARNESS_KINDS } from '../../../shared/runWithAgent/shared';
import { localRunnerApi } from '../api';

export interface TestRunStepProps {
  thisMachineRunner: RunnerSummary | null;
}

/**
 * One button, no fields: runs the first installed agent on this computer
 * through `POST /api/local-runner/test-run` and shows the timeline of the
 * request it returns.
 */
const TestRunStep: Component<TestRunStepProps> = (props) => {
  const store = useExecutionStore();
  const [requestId, setRequestId] = createSignal<string | null>(null);
  const [running, setRunning] = createSignal(false);

  const installed = (kind: string) => {
    const h = findHarness(harnessesOf(props.thisMachineRunner), kind);
    return isReady(h) ? h : undefined;
  };
  // The test run names no model, and the scheduler refuses that for an agent
  // that reports it needs one: the request would wait with nothing to run it.
  const candidates = () =>
    HARNESS_KINDS.map((k) => k.value).filter((kind) => installed(kind) && !installed(kind)?.requires_model);
  // An agent known to be signed out would only fail: the test uses one that is not, and when none is, it says so.
  const signedOut = (kind: string) => installed(kind)?.sign_in?.state === 'signed_out';
  const harnessKind = () => candidates().find((kind) => !signedOut(kind)) ?? candidates()[0];
  const blockedBySignIn = () => {
    const kind = harnessKind();
    return kind !== undefined && signedOut(kind) ? kind : undefined;
  };
  const needsModelKind = () => HARNESS_KINDS.map((k) => k.value).find((kind) => installed(kind));
  const itemId = createMemo(() => {
    const id = requestId();
    return id ? store.getRequest(id)?.summary?.item_id : undefined;
  });

  const run = async () => {
    const kind = harnessKind();
    if (!kind || blockedBySignIn()) return;
    setRunning(true);
    try {
      const { request_id } = await localRunnerApi.testRun(kind);
      setRequestId(request_id);
      await store.loadOne(request_id);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to start the test run');
    } finally {
      setRunning(false);
    }
  };

  return (
    <section class="flex flex-col gap-3 rounded-card bg-panel px-[22px] py-5">
      <h2 class="text-[1.25rem] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
        Test run
      </h2>
      <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
        Runs {harnessKind() ?? 'an installed agent'} once on this computer, with its own login, and shows what happened.
      </p>
      <Button class="self-start" onClick={() => void run()} loading={running()} disabled={running() || !harnessKind() || !!blockedBySignIn()}>
        Run test
      </Button>
      <Show when={blockedBySignIn()}>
        {(kind) => (
          <p data-testid="test-run-signed-out" class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
            {HARNESS_KINDS.find((k) => k.value === kind())?.label ?? kind()} is not signed in
            <Show when={installed(kind())?.sign_in?.command}>{(command) => <>: run <code class="font-mono">{command()}</code> in a terminal, then re-check</>}</Show>
            . The test would fail before it started.
          </p>
        )}
      </Show>
      <Show when={!harnessKind()}>
        <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
          <Show
            when={needsModelKind()}
            fallback="No installed agent to test — turn on agent execution and install one above first."
          >
            {(kind) => `${kind()} needs a model named for every run, and the test run names none. Run it from a task with a specific model instead.`}
          </Show>
        </p>
      </Show>
      <Show when={itemId()}>{(id) => <ExecutionTimeline itemId={id()} />}</Show>
    </section>
  );
};

export default TestRunStep;
