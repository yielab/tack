import { type Component, Show, createMemo, createSignal } from 'solid-js';
import { toast } from '../../../shared/ui/toast';
import { Button } from '../../../shared/ui';
import type { RunnerSummary } from '../../../shared/execution/api';
import { useExecutionStore } from '../../../shared/state/executionContext';
import ExecutionTimeline from '../../../shared/runWithAgent/ExecutionTimeline';
import { findHarness, harnessesOf } from '../runnerObservations';
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

  const harnessKind = () =>
    HARNESS_KINDS.map((k) => k.value).find((kind) => {
      const h = findHarness(harnessesOf(props.thisMachineRunner), kind);
      return h && h.probe_error === null && !!h.installed_version;
    });
  const itemId = createMemo(() => {
    const id = requestId();
    return id ? store.getRequest(id)?.summary?.item_id : undefined;
  });

  const run = async () => {
    const kind = harnessKind();
    if (!kind) return;
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
    <section class="flex flex-col gap-3 rounded-[28px] bg-panel px-[22px] py-5">
      <h2 class="text-[20px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
        Test run
      </h2>
      <p class="text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
        Runs {harnessKind() ?? 'an installed agent'} once on this computer, with its own login, and shows what happened.
      </p>
      <Button class="self-start" onClick={() => void run()} loading={running()} disabled={running() || !harnessKind()}>
        Run test
      </Button>
      <Show when={!harnessKind()}>
        <p class="text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
          No installed agent to test — turn on agent execution and install one above first.
        </p>
      </Show>
      <Show when={itemId()}>{(id) => <ExecutionTimeline itemId={id()} />}</Show>
    </section>
  );
};

export default TestRunStep;
