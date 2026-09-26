import { type Component, type JSX, Show, createEffect, createResource, createSignal, onCleanup, onMount } from 'solid-js';
import { api } from '../../shared/api';
import { Select } from '../../shared/ui';
import { runnersApi } from '../../shared/execution';
import { localRunnerApi, isLocalRunnerUnavailable } from './api';
import ExecutionToggle from './ExecutionToggle';
import HarnessStep from './steps/HarnessStep';
import ProviderStep from './steps/ProviderStep';
import ModelDefaultStep from './steps/ModelDefaultStep';
import TestRunStep from './steps/TestRunStep';
import AdvancedSection from './AdvancedSection';
import { countOtherActiveRunners, findThisMachineRunner, harnessesOf, isThisMachineRunner } from './runnerObservations';
import { getRememberedAgentsProjectId, setRememberedAgentsProjectId } from './agentsProjectPreference';

/** How often this page re-polls `GET /api/runners` while mounted — enough
 *  to notice a toggle/re-check without the operator reloading, without
 *  hammering the endpoint. `ExecutionToggle` manages its own separate
 *  fetch of `/api/local-runner` with no way for a sibling to subscribe to
 *  it, so this is a plain poll rather than an event. */
const RUNNERS_POLL_MS = 1500;

type StepState = 'done' | 'current' | 'pending';

/** One row of the numbered stepper: a round marker (check when its step's
 *  own observation says done, accent ring on the first step that isn't,
 *  divider ring after that), a connector down to the next marker, and the
 *  step's own card beside it. The marker is decorative — each step's own
 *  heading and badge carry the text. */
const StepRow: Component<{ number: number; state: StepState; last?: boolean; children: JSX.Element }> = (props) => {
  const markerStyle = (): JSX.CSSProperties => {
    switch (props.state) {
      case 'done':
        return { background: 'var(--color-success-600)', color: 'var(--color-bg-app)' };
      case 'current':
        return {
          background: 'var(--color-bg-app)',
          border: '3px solid var(--color-primary-600)',
          color: 'var(--color-accent-ink)',
        };
      default:
        return {
          background: 'var(--color-bg-app)',
          border: '3px solid var(--color-border-light)',
          color: 'var(--color-text-secondary)',
        };
    }
  };
  return (
    <div class="grid grid-cols-[44px_minmax(0,1fr)] gap-4" data-step-state={props.state}>
      <div class="flex flex-col items-center" aria-hidden="true">
        <span
          class="grid h-11 w-11 flex-none place-items-center rounded-full text-[18px]"
          classList={{ 'font-heading': props.state !== 'done' }}
          style={markerStyle()}
        >
          {props.state === 'done' ? '✓' : props.number}
        </span>
        <Show when={!props.last}>
          <span
            class="mt-1.5 w-[3px] flex-1 rounded-sm"
            style={{
              background:
                props.state === 'done'
                  ? 'color-mix(in srgb, var(--color-success-600) 40%, transparent)'
                  : 'var(--color-border-light)',
            }}
          />
        </Show>
      </div>
      <div class="min-w-0">{props.children}</div>
    </div>
  );
};

/**
 * The Agents page — five numbered, API-observed steps from an installed
 * binary to a completed attempt, plus an Advanced section for running
 * agents on other machines. Composes `ExecutionToggle`/`ProviderKeyPanel`
 * (each independently fetches and owns its own state) with the harness,
 * provider-login, default-model and test-run steps, and the `runnerFleet/`
 * management tree.
 */
const AgentsPage: Component = () => {
  const [runnersResult, { refetch: refetchRunners }] = createResource(() => runnersApi.list());
  const [localRunnerStatus, { refetch: refetchLocalRunnerStatus }] = createResource(() => localRunnerApi.get());
  const localRunnerUnavailable = () => isLocalRunnerUnavailable(localRunnerStatus.error);

  // Turning the embedded runner off does not revoke or remove its
  // enrollment row (confirmed live: `state` stays `active`, only
  // `last_heartbeat_at` freezes) — so "this machine's own row" is only
  // trustworthy while `/api/local-runner` itself reports `running`.
  // Filtering it out otherwise, once, here keeps every step below (harness
  // list, provider logins, model-default union, test-run target) from
  // separately re-deriving the same check.
  const runners = () => {
    const all = runnersResult()?.data.data ?? [];
    if (localRunnerStatus()?.state === 'running') return all;
    return all.filter((r) => !isThisMachineRunner(r));
  };
  const thisMachineRunner = () => findThisMachineRunner(runners());
  const otherActiveCount = () => countOtherActiveRunners(runners());

  let pollHandle: ReturnType<typeof setInterval> | undefined;
  onMount(() => {
    pollHandle = setInterval(() => {
      void refetchRunners();
      void refetchLocalRunnerStatus();
    }, RUNNERS_POLL_MS);
  });
  onCleanup(() => {
    if (pollHandle) clearInterval(pollHandle);
  });

  const [rechecking, setRechecking] = createSignal(false);
  const recheck = async () => {
    setRechecking(true);
    try {
      await localRunnerApi.update(false);
      await localRunnerApi.update(true);
      await Promise.all([refetchRunners(), refetchLocalRunnerStatus()]);
    } finally {
      setRechecking(false);
    }
  };

  // Step 4/5 need a project; this route carries no `:id` (it is a global
  // page, not nested under `/projects/:id`), so it keeps its own
  // selection, remembered per browser.
  const [projects] = createResource(() => api.projects.list());
  const [selectedProjectId, setSelectedProjectId] = createSignal('');
  createEffect(() => {
    if (selectedProjectId()) return;
    const list = projects();
    if (!list || list.length === 0) return;
    const remembered = getRememberedAgentsProjectId();
    setSelectedProjectId(remembered && list.some((p) => p.id === remembered) ? remembered : list[0].id);
  });
  createEffect(() => {
    const id = selectedProjectId();
    if (id) setRememberedAgentsProjectId(id);
  });
  const [project, { refetch: refetchProject }] = createResource(selectedProjectId, (id) =>
    id ? api.projects.get(id) : null,
  );

  const [verifiedHarnessKinds, setVerifiedHarnessKinds] = createSignal<ReadonlySet<string>>(new Set());
  const markVerified = (harnessKind: string) => {
    setVerifiedHarnessKinds((prev) => new Set(prev).add(harnessKind));
  };

  // Each marker reads the same observations its step already renders —
  // nothing here is a separate notion of progress. Step 3 and step 5 both
  // flip only on a test run this tab watched succeed (the one signal that
  // turns step 3's "Present, unverified" badge into "Verified").
  const stepDone = (): boolean[] => {
    const verified = verifiedHarnessKinds().size > 0;
    return [
      localRunnerStatus.error === undefined && localRunnerStatus()?.state === 'running',
      harnessesOf(thisMachineRunner()).some((h) => h.probe_error === null && !!h.installed_version),
      verified,
      !!project()?.default_model,
      verified,
    ];
  };
  const stepState = (index: number): StepState => {
    const done = stepDone();
    if (done[index]) return 'done';
    return done.findIndex((d) => !d) === index ? 'current' : 'pending';
  };

  return (
    <div class="max-w-[780px] space-y-[22px]">
      <div>
        <h1 class="text-[38px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
          Agents
        </h1>
        <p class="mt-1.5 max-w-[620px] text-[15px]" style={{ color: 'var(--color-text-secondary)' }}>
          Turn on an agent, give it a model provider, and run a test — the whole path from
          an installed binary to a completed run.
        </p>
      </div>

      <Show when={(projects()?.length ?? 0) > 1 || (!localRunnerUnavailable() && otherActiveCount() > 0)}>
        <div class="flex flex-wrap items-end gap-3">
          <Show when={(projects()?.length ?? 0) > 1}>
            <div class="w-[260px] max-w-full">
              <Select
                label="Project"
                value={selectedProjectId()}
                onChange={(e) => setSelectedProjectId(e.currentTarget.value)}
                options={(projects() ?? []).map((p) => ({ value: p.id, label: p.name }))}
              />
            </div>
          </Show>
          <Show when={!localRunnerUnavailable() && otherActiveCount() > 0}>
            <p
              class="rounded-full px-3.5 py-2 text-[13px]"
              style={{ background: 'var(--color-accent2-soft)', color: 'var(--color-accent2-ink)' }}
            >
              {otherActiveCount()} other machine{otherActiveCount() === 1 ? ' is' : 's are'} running agents —{' '}
              <a href="#advanced" class="font-bold underline" style={{ color: 'inherit' }}>see Advanced</a>.
            </p>
          </Show>
        </div>
      </Show>

      <div class="space-y-[22px]">
        <StepRow number={1} state={stepState(0)}>
          <ExecutionToggle />
        </StepRow>

        <StepRow number={2} state={stepState(1)}>
          <HarnessStep thisMachineRunner={thisMachineRunner()} onRecheck={() => void recheck()} rechecking={rechecking()} />
        </StepRow>

        <StepRow number={3} state={stepState(2)}>
          <ProviderStep thisMachineRunner={thisMachineRunner()} verifiedHarnessKinds={verifiedHarnessKinds()} />
        </StepRow>

        <StepRow number={4} state={stepState(3)}>
          <ModelDefaultStep project={project()} runners={runners()} onSaved={() => void refetchProject()} />
        </StepRow>

        <StepRow number={5} state={stepState(4)} last>
          <TestRunStep project={project()} runners={runners()} onVerified={markVerified} />
        </StepRow>
      </div>

      <AdvancedSection />
    </div>
  );
};

export default AgentsPage;
