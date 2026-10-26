import { type Component, For, type JSX, Show, createEffect, createMemo, createResource, createSignal, on, onCleanup, onMount } from 'solid-js';
import { Badge, Button } from '../ui';
import { toast } from '../ui/toast';
import { useExecutionStore } from '../state/executionContext';
import { artifactsApi, attemptsApi, type AttemptSummary } from '../execution';
import {
  NOT_MEASURED_TEXT,
  changedNoFile,
  countPatchFiles,
  describeAttemptOutcome,
  describeAttemptStatus,
  formatApproxCost,
  formatCountedTokens,
  formatWallClock,
  isTerminalAttemptState,
  lastSteps,
  needsVerdict,
  readAttemptUsage,
} from './attemptFormat';
import MergeButton from './MergeButton';
import ReviewActions from './ReviewActions';
import RerunAtL3, { needsAPerson } from './RerunAtL3';
import RunLimitNote from './RunLimitNote';
import RunWithAgentModal from './RunWithAgentModal';
import { parseRunLog } from './runLog';
import { stoppedByLimit } from './runLimit';
import { describeExecutionState, isTerminalStateString, relativeTimeFromIso, type StateTone } from './shared';

export interface LatestRunSummaryProps {
  itemId: string;
  /** With the task's title and project, a run its limit stopped offers "Run again with more". */
  itemTitle?: string;
  projectId?: string;
  onCreated?: (requestId: string) => void;
  /** Opens the run's full report (the item's Execution tab). */
  onOpen: () => void;
  /** The Execution tab is open and asks for the verdict in its own banner, so the card leaves
   *  out Accept and Reject… and keeps its state, its line and its other links. */
  reviewInTab?: boolean;
}

/** Which card the run is: it sets the card's colour and its actions. */
type RunCardKind = 'queued' | 'running' | 'review' | 'failed' | 'merged' | 'done';

const EXCERPT_CHARS = 220;
const ANSWER_CHARS = 600;
const LIVE_RELOAD_MS = 3000;

const excerpt = (text: string, max: number) => {
  const flat = text.trim().replace(/\s+/g, ' ');
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
};

/** The item's latest run as one card whose colour and actions follow the run: its last steps
 *  and Stop while it runs; the opening of its report and Accept / Reject… when it wants a
 *  verdict; why it failed; that it merged; why it still waits. Renders nothing until the
 *  item has a run. */
const LatestRunSummary: Component<LatestRunSummaryProps> = (props) => {
  const store = useExecutionStore();
  onMount(() => onCleanup(store.watchItem(props.itemId)));
  const record = createMemo(() => store.requestsForItem(props.itemId)[0]);
  const latest = createMemo(() => {
    const r = record();
    return r?.status === 'ready' ? r.summary : undefined;
  });
  const attempts = createMemo(() => store.attemptsFor(latest()?.request_id ?? ''));
  const reload = () => {
    const id = latest()?.request_id;
    if (id) void store.loadAttempts(id).catch(() => undefined);
  };
  createEffect(() => {
    if (latest() && attempts().status === 'idle') reload();
  });
  const attempt = createMemo<AttemptSummary | undefined>(() => {
    const a = attempts();
    if (a.status !== 'ready') return undefined;
    return a.data.reduce<AttemptSummary | undefined>((max, x) => (!max || x.attempt_number > max.attempt_number ? x : max), undefined);
  });
  // The verdict as last saved here, until the reloaded attempt carries it.
  const [saved, setSaved] = createSignal<AttemptSummary['review'] | undefined>(undefined);
  const review = () => (saved() !== undefined ? saved() : attempt()?.review);
  createEffect(on(() => attempt()?.attempt_id, () => setSaved(undefined), { defer: true }));
  const finished = () => {
    const a = attempt();
    return !!a && isTerminalAttemptState(a.state);
  };
  const noChange = () => !!attempt() && changedNoFile(attempt()!);
  const mergedBy = () => attempt()?.merge?.merged ?? null;
  const isMerged = () => !!mergedBy() || attempt()?.pull_request?.state === 'merged';

  const kind = (): RunCardKind => {
    const run = latest();
    const a = attempt();
    if (run?.state === 'queued') return 'queued';
    if (!a) return run && isTerminalStateString(run.state) ? (run.state === 'failed' || run.state === 'cancelled' ? 'failed' : 'done') : 'running';
    if (!isTerminalAttemptState(a.state)) return 'running';
    if (a.state !== 'succeeded') return 'failed';
    if (isMerged()) return 'merged';
    return needsVerdict(a) && !review() ? 'review' : 'done';
  };
  const status = (): { label: string; tone: StateTone } => {
    const a = attempt();
    if (kind() === 'merged') return { label: 'Merged', tone: 'success' };
    if (a) return describeAttemptStatus(a, review());
    const s = describeExecutionState(latest()?.state ?? 'queued');
    return { label: s.label, tone: s.tone };
  };
  const background = () => {
    switch (kind()) {
      case 'running': return 'var(--color-accent-soft)';
      case 'review': return 'var(--color-warning-100)';
      case 'merged': return 'var(--color-success-100)';
      case 'failed': return status().tone === 'danger' ? 'var(--color-danger-100)' : 'var(--color-bg-subtle)';
      case 'done': return status().tone === 'success' ? 'var(--color-success-100)' : 'var(--color-bg-app)';
      default: return 'var(--color-bg-subtle)';
    }
  };

  // What the agent said at the end: the opening of its report, or, for a run that changed
  // no file, the answer it delivered.
  const said = () => {
    const result = (attempt()?.terminal_reason as { result?: unknown } | null)?.result;
    if (typeof result !== 'string' || result.trim() === '') return null;
    return excerpt(result, noChange() ? ANSWER_CHARS : EXCERPT_CHARS);
  };
  const whyStopped = () => {
    const a = attempt();
    return a ? describeAttemptOutcome(a.state, a.terminal_reason).whyStopped : null;
  };
  const usageLine = () => {
    const a = attempt();
    if (!a || !finished()) return null;
    const u = readAttemptUsage(a);
    const parts: string[] = [];
    const tokens = formatCountedTokens(u);
    if (tokens !== null) parts.push(tokens);
    const cost = formatApproxCost(u.cost);
    if (cost !== NOT_MEASURED_TEXT) parts.push(cost);
    if (u.modelCalls !== null) parts.push(`${u.modelCalls} model ${u.modelCalls === 1 ? 'call' : 'calls'}`);
    if (u.models.length > 0) parts.push(u.models[0]);
    return parts.length > 0 ? parts.join(' · ') : null;
  };
  const failedReason = () => (latest()?.state === 'failed' ? latest()?.waiting_reason || null : null);
  const waitingReason = () => (latest()?.state === 'queued' ? latest()?.waiting_reason || null : null);

  // Duration, steps and files changed, read off the finished run's own artifacts.
  const [digest] = createResource(
    () => {
      const a = attempt();
      return a && a.state === 'succeeded' ? { id: a.request_id, n: a.attempt_number } : undefined;
    },
    async ({ id, n }) => {
      const list = await artifactsApi.list(id, n).catch(() => []);
      const log = list.find((x) => x.kind === 'log' || (x.name ?? '').endsWith('.log'));
      const patch = list.find((x) => x.kind === 'patch' && x.size_bytes > 0);
      const text = async (x: { artifact_id: string }) => (await artifactsApi.download(id, n, x.artifact_id)).text();
      const [steps, files] = await Promise.all([
        log ? text(log).then((t) => parseRunLog(t).steps.length).catch(() => null) : null,
        patch ? text(patch).then(countPatchFiles).catch(() => null) : null,
      ]);
      return { steps, files };
    },
  );
  const metaLine = () => {
    const a = attempt();
    if (!a || !finished()) return null;
    const parts: string[] = [];
    const ms = readAttemptUsage(a).wallClockMs;
    if (ms !== null) parts.push(formatWallClock(ms, true));
    const d = digest();
    if (d?.steps) parts.push(`${d.steps} ${d.steps === 1 ? 'step' : 'steps'}`);
    if (noChange()) parts.push('no files changed');
    else if (d?.files) parts.push(`${d.files} ${d.files === 1 ? 'file' : 'files'} changed`);
    return parts.length > 0 ? parts.join(' · ') : null;
  };

  // While it runs: its last steps, reloaded every few seconds.
  const live = () => kind() === 'running' && attempt() !== undefined;
  const [events, { refetch }] = createResource(
    () => (live() ? { id: attempt()!.request_id, n: attempt()!.attempt_number } : undefined),
    ({ id, n }) => attemptsApi.events(id, n).then((r) => r.data.data).catch(() => []),
  );
  createEffect(() => {
    if (!live()) return;
    const timer = setInterval(() => void refetch(), LIVE_RELOAD_MS);
    onCleanup(() => clearInterval(timer));
  });
  const steps = () => lastSteps(events.latest ?? []);

  const [stopping, setStopping] = createSignal(false);
  const stop = async () => {
    const id = latest()?.request_id;
    if (!id) return;
    setStopping(true);
    try {
      await store.cancel(id);
      toast.success('Stop requested.');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to request cancellation.');
    } finally {
      setStopping(false);
    }
  };
  const stopRequested = () => !!record()?.cancellation.requested;

  // A queued run that waits for a computer is replaced by a run the person points elsewhere.
  const [another, setAnother] = createSignal(false);
  const canPickAnother = () => !!props.itemTitle && !!props.projectId;

  const muted = (extra?: JSX.CSSProperties): JSX.CSSProperties => ({ color: 'var(--color-text-secondary)', ...extra });
  const open = (label: string) => (
    <Button size="sm" variant="secondary" onClick={() => props.onOpen()}>
      {label}
    </Button>
  );

  return (
    <Show when={latest()}>
      {(run) => (
        <div
          data-testid="latest-run"
          data-state={kind()}
          class="min-w-0 space-y-2 rounded-[20px] p-4"
          style={{ 'background-color': background(), 'box-shadow': 'var(--shadow-sm)' }}
        >
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>Last run</span>
            <Badge tone={status().tone}>{status().label}</Badge>
            <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              {relativeTimeFromIso(attempt()?.ended_at ?? attempt()?.started_at ?? run().created_at)}
            </span>
          </div>

          <Show when={kind() === 'queued'}>
            <p data-testid="latest-run-waiting" class="text-sm" style={muted()}>
              {waitingReason() ? `Waiting: ${waitingReason()}` : 'Waiting for a runner to pick it up.'}
              <Show when={waitingReason() && latest()?.waiting_since}>
                {(since) => <span style={{ color: 'var(--color-text-tertiary)' }}> · first seen {relativeTimeFromIso(since())}</span>}
              </Show>
            </p>
          </Show>

          <Show when={kind() === 'running'}>
            <Show
              when={steps().length > 0}
              fallback={
                <p class="text-sm" style={muted()}>
                  {(attempt()?.state ?? run().state) === 'waiting_decision'
                    ? 'The agent is waiting for your answer.'
                    : 'The agent is working on it.'}
                </p>
              }
            >
              <ol data-testid="latest-run-steps" class="space-y-0.5 text-sm" style={muted()}>
                <For each={steps()}>{(line) => <li class="break-words">{line}</li>}</For>
              </ol>
            </Show>
          </Show>

          <Show when={kind() === 'failed'}>
            <Show
              when={whyStopped() ?? failedReason()}
              fallback={<p class="text-sm" style={muted()}>The run did not finish.</p>}
            >
              {(why) => (
                <p data-testid={whyStopped() ? 'latest-run-why' : 'latest-run-failure'} class="text-sm" style={{ color: 'var(--color-danger-600)' }}>
                  {why()}
                </p>
              )}
            </Show>
          </Show>

          <Show when={kind() === 'merged'}>
            <p data-testid="latest-run-merged" class="text-sm" style={muted()}>
              {mergedBy() ? `Merged ${relativeTimeFromIso(mergedBy()!.merged_at)}` : 'Its pull request is merged.'}
            </p>
          </Show>

          <Show when={kind() === 'review' || kind() === 'done' || kind() === 'merged'}>
            <Show when={whyStopped()}>
              {(why) => <p data-testid="latest-run-why" class="text-sm" style={{ color: 'var(--color-danger-600)' }}>{why()}</p>}
            </Show>
            <Show when={said()}>
              {(text) => (
                <>
                  <Show when={noChange()}>
                    <p class="text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>What it delivered</p>
                  </Show>
                  <p data-testid="latest-run-said" class="text-sm" style={muted()}>“{text()}”</p>
                </>
              )}
            </Show>
          </Show>

          <Show when={metaLine()}>
            {(line) => (
              <p data-testid="latest-run-meta" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>{line()}</p>
            )}
          </Show>
          <Show when={usageLine()}>
            {(line) => (
              <p data-testid="latest-run-usage" class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                {line()}
              </p>
            )}
          </Show>
          <Show when={latest()?.limit}>
            {(limit) => (
              <RunLimitNote
                limit={limit()}
                running={kind() === 'running'}
                stopped={kind() === 'failed' && stoppedByLimit(attempt()?.terminal_reason)}
                testId="latest-run-limit"
                itemId={props.itemId}
                itemTitle={props.itemTitle}
                projectId={props.projectId}
                onCreated={props.onCreated}
              />
            )}
          </Show>

          <Show when={kind() === 'failed' && needsAPerson(attempt())}>
            <RerunAtL3
              attempt={attempt()!}
              itemId={props.itemId}
              itemTitle={props.itemTitle}
              projectId={props.projectId}
              onCreated={props.onCreated}
            />
          </Show>

          <div class="flex flex-wrap items-center gap-2 pt-1">
            <Show when={kind() === 'running'}>
              {open('Follow live')}
              <Button size="sm" variant="secondary" disabled={stopping() || stopRequested()} loading={stopping()} onClick={() => void stop()}>
                {stopRequested() ? 'Stopping…' : 'Stop'}
              </Button>
            </Show>
            <Show when={kind() === 'queued'}>
              <Button
                size="sm"
                variant="secondary"
                disabled={!canPickAnother()}
                title={canPickAnother() ? undefined : 'This screen does not know the task'}
                onClick={() => setAnother(true)}
              >
                Run on another computer
              </Button>
              <Show when={canPickAnother()}>
                <RunWithAgentModal
                  isOpen={another()}
                  onClose={() => setAnother(false)}
                  itemId={props.itemId}
                  itemTitle={props.itemTitle!}
                  projectId={props.projectId!}
                  onCreated={(requestId) => {
                    void store.cancel(run().request_id).catch(() => undefined);
                    props.onCreated?.(requestId);
                  }}
                />
              </Show>
            </Show>
            <Show when={!props.reviewInTab && attempt() && (kind() === 'review' || (kind() === 'done' && !!review()))}>
              <ReviewActions
                requestId={attempt()!.request_id}
                attemptNumber={attempt()!.attempt_number}
                review={review()}
                onReviewed={(r) => {
                  setSaved(r);
                  reload();
                }}
              />
            </Show>
            <Show when={kind() === 'review' || kind() === 'merged' || kind() === 'done' || kind() === 'failed'}>
              {open('See what it did')}
            </Show>
          </div>

          <Show when={finished() && !noChange() && kind() !== 'failed' && !isMerged() && attempt()?.merge}>
            {(merge) => <MergeButton attemptId={attempt()!.attempt_id} merge={merge()} onMerged={reload} />}
          </Show>
        </div>
      )}
    </Show>
  );
};

export default LatestRunSummary;
