import { type Component, For, type JSX, Show, createEffect, createMemo, createResource, createSignal, createUniqueId, on, onCleanup, onMount } from 'solid-js';
import { Button, LoadError, RunState, type DocketRunWord, type RunStateName } from '../ui';
import { toast } from '../ui/toast';
import { useExecutionStore } from '../state/executionContext';
import { artifactsApi, attemptsApi, folderRunSummary, isFolderRun, readRunSteps, type AttemptSummary } from '../execution';
import type { WaitingBehind } from '../execution/api';
import {
  changedNoFile,
  countPatchFiles,
  describeAttemptOutcome,
  describeAttemptStatus,
  formatCountedTokens,
  formatWallClock,
  isTerminalAttemptState,
  lastSteps,
  needsVerdict,
  readAttemptUsage,
} from './attemptFormat';
import DecisionInbox from './DecisionInbox';
import MergeButton from './MergeButton';
import ReviewActions from './ReviewActions';
import RerunAtL3, { needsAPerson } from './RerunAtL3';
import RunLimitNote from './RunLimitNote';
import RunWithAgentButton from './RunWithAgentButton';
import RunWithAgentModal from './RunWithAgentModal';
import TeamStepsLine, { runWho } from './TeamSteps';
import UsageMeter from './UsageMeter';
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
  /** Told the run's state whenever it changes, and `undefined` while the item has no run, so the
   *  screen can mark the Execution tab with the same state. */
  onState?: (state: { state: RunStateName; docket?: DocketRunWord } | undefined) => void;
  /** Under the Run button of an item with no run: who would do it. */
  idle?: JSX.Element;
  /** The panel's last line: what happens to the item without the person. It brings its own hairline. */
  footer?: JSX.Element;
}

/** Which card the run is: it sets the card's state and its actions. */
export type RunCardKind = 'queued' | 'running' | 'review' | 'failed' | 'merged' | 'done';

/** The one filled button of the panel, chosen by where the item is (Run, Answer the question,
 *  Review and merge, See why it failed, Run again); null when nothing needs doing now. */
export type NowAction = 'run' | 'answer' | 'review' | 'why' | 'again';

export function primaryNowAction(input: {
  hasRun: boolean;
  kind: RunCardKind;
  asking: boolean;
  askingBehind: boolean;
  /** The run was stopped on purpose: there is nothing to explain, so what to do next is to run it again. */
  cancelled?: boolean;
}): NowAction | null {
  if (!input.hasRun) return 'run';
  switch (input.kind) {
    case 'queued': return input.askingBehind ? 'answer' : null;
    case 'running': return input.asking ? 'answer' : null;
    case 'review': return 'review';
    case 'failed': return input.cancelled ? 'again' : 'why';
    case 'merged':
    case 'done': return 'again';
  }
}

/** Status words the banner already says, so the line under it does not repeat them. */
const SAID_BY_BANNER = new Set(['Queued', 'Running', 'Failed', 'Cancelled', 'Needs a person']);

const EXCERPT_CHARS = 220;
const ANSWER_CHARS = 600;
const LIVE_RELOAD_MS = 3000;

const excerpt = (text: string, max: number) => {
  const flat = text.trim().replace(/\s+/g, ' ');
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
};

/** The item's Now panel: its latest run as one card whose state and actions follow the run (its
 *  last steps and Stop while it runs; the opening of its report and Accept / Reject… when it wants
 *  a verdict, with Accept / Reject… only when there is no pack to review; why it failed; that it merged; why it still waits), with one filled button for what
 *  to do next. An item with no run shows Run with agent instead. */
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
  // The run in the one language every screen uses; the docket words stay as the banner's second line.
  const runState = (): { state: RunStateName; docket?: DocketRunWord } => {
    const raw = attempt()?.state ?? latest()?.state;
    switch (kind()) {
      case 'queued': return { state: 'queued' };
      case 'running':
        if (asking()) return { state: 'needs', docket: 'Needs a person' };
        return stopRequested() ? { state: 'running', docket: 'Cancellation requested' } : { state: 'running' };
      case 'review': return { state: 'review', docket: 'Needs your review' };
      case 'failed':
        if (needsPerson()) return { state: 'needs', docket: 'Needs a person' };
        return { state: raw === 'cancelled' ? 'cancelled' : 'failed' };
      case 'merged': return { state: 'done' };
      case 'done': return noChange() && !folder() ? { state: 'done', docket: 'no changes recorded' } : { state: 'done' };
    }
  };
  // The words under the banner: what the status adds to it, and when it last changed.
  const statusLine = () => {
    const label = status().label;
    const when = relativeTimeFromIso(attempt()?.ended_at ?? attempt()?.started_at ?? latest()?.created_at ?? '');
    return [SAID_BY_BANNER.has(label) ? null : label, when].filter(Boolean).join(' · ');
  };

  // What the agent said at the end: the opening of its report, or, for a run that changed
  // no file, the answer it delivered.
  // A folder team's run says what its last step did instead, and has nothing to merge.
  const folder = () => !!attempt() && isFolderRun(attempt()!.terminal_reason);
  const said = () => {
    const reason = attempt()?.terminal_reason;
    const result = folder() ? folderRunSummary(reason) : (reason as { result?: unknown } | null | undefined)?.result;
    if (typeof result !== 'string' || result.trim() === '') return null;
    return excerpt(result, noChange() ? ANSWER_CHARS : EXCERPT_CHARS);
  };
  // The questions the agent asked across its steps: one line that opens the tab.
  const questionCount = () => (readRunSteps(attempt()?.terminal_reason) ?? []).reduce((n, step) => n + step.questions.length, 0);
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
    if (u.modelCalls !== null) parts.push(`${u.modelCalls} model ${u.modelCalls === 1 ? 'call' : 'calls'}`);
    if (u.models.length > 0) parts.push(u.models[0]);
    return parts.length > 0 ? parts.join(' · ') : null;
  };
  // The agent stopped to ask a person, or ended because it needed one: one state, with the question one press away.
  const asking = () => (attempt()?.state ?? latest()?.state) === 'waiting_decision';
  const needsPerson = () => asking() || needsAPerson(attempt());
  const failedReason = () => (latest()?.state === 'failed' ? latest()?.waiting_reason || null : null);
  const waitingReason = () => (latest()?.state === 'queued' ? latest()?.waiting_reason || null : null);
  // When the wait is a full computer, the sentence names its limit and the tasks that hold it:
  // each becomes a link, in the order the sentence says them.
  const behind = () => (latest()?.state === 'queued' ? latest()?.waiting_behind ?? [] : []);
  const itemHref = (b: WaitingBehind, tab = '') => `/projects/${b.project_id}/items/${b.item_id}${tab}`;
  const waitingParts = (): Array<{ text: string; href?: string }> => {
    const reason = waitingReason() ?? '';
    const links: Array<{ needle: string; href: string }> = [];
    const seen = new Set<string>();
    for (const b of behind()) {
      if (!seen.has(b.runner_id)) {
        seen.add(b.runner_id);
        links.push({ needle: `runs ${b.runs_at_a_time} at a time`, href: '/agents' });
      }
      links.push({ needle: `'${b.item_title}'`, href: itemHref(b) });
    }
    const parts: Array<{ text: string; href?: string }> = [];
    let at = 0;
    for (const { needle, href } of links) {
      const found = reason.indexOf(needle, at);
      if (found < 0) continue;
      if (found > at) parts.push({ text: reason.slice(at, found) });
      parts.push({ text: needle, href });
      at = found + needle.length;
    }
    if (at < reason.length) parts.push({ text: reason.slice(at) });
    return parts;
  };
  // The first busy run that waits for a person: its question is one press away.
  const askingBehind = () => behind().find((b) => b.question !== null);

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
    // A folder team has no repository: its card already says "nothing to merge".
    if (noChange()) { if (!folder()) parts.push('no files changed'); }
    else if (d?.files) parts.push(`${d.files} ${d.files === 1 ? 'file' : 'files'} changed`);
    return parts.length > 0 ? parts.join(' · ') : null;
  };

  // Whether the run left a merge-readiness pack: with one, the verdict is given on the Execution tab, where
  // the pack and its reason are reviewed together, so this panel offers no Accept or Reject of its own.
  const [hasPack] = createResource(
    () => {
      const a = attempt();
      return a && kind() === 'review' ? { id: a.request_id, n: a.attempt_number } : undefined;
    },
    async ({ id, n }) => {
      try {
        return (await artifactsApi.list(id, n)).some((x) => x.kind === 'mrp');
      } catch {
        return false;
      }
    },
  );
  // Known to have no pack: only then are Accept and Reject… shown here.
  const inlineReview = () => !hasPack.loading && hasPack() === false;

  // While it runs: its last steps, reloaded every few seconds.
  const live = () => kind() === 'running' && attempt() !== undefined;
  const [events, { refetch }] = createResource(
    () => (live() ? { id: attempt()!.request_id, n: attempt()!.attempt_number } : undefined),
    ({ id, n }) => attemptsApi.events(id, n).then((r) => r.data.data),
  );
  createEffect(() => {
    if (!live()) return;
    const timer = setInterval(() => void refetch(), LIVE_RELOAD_MS);
    onCleanup(() => clearInterval(timer));
  });
  // A failed reload keeps the steps already read and says so; it never empties them.
  const eventsFailed = () => events.state === 'errored';
  const steps = () => lastSteps(events.state === 'errored' ? [] : (events.latest ?? []));

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

  createEffect(() => props.onState?.(latest() ? runState() : undefined));

  const headingId = createUniqueId();
  const muted = (extra?: JSX.CSSProperties): JSX.CSSProperties => ({ color: 'var(--color-text-secondary)', ...extra });
  const open = (label: string) => (
    <Button size="sm" variant="secondary" onClick={() => props.onOpen()}>
      {label}
    </Button>
  );
  const openPrimary = (label: string) => <Button size="md" data-testid="now-primary" onClick={() => props.onOpen()}>{label}</Button>;
  const primary = () =>
    primaryNowAction({ hasRun: !!latest(), kind: kind(), asking: asking(), askingBehind: !!askingBehind(), cancelled: runState().state === 'cancelled' });
  const runAgain = (variant: 'primary' | 'ghost') => (
    <RunWithAgentButton
      itemId={props.itemId}
      itemTitle={props.itemTitle ?? ''}
      projectId={props.projectId ?? ''}
      variant={variant}
      testId={variant === 'primary' ? 'now-primary' : undefined}
      onCreated={props.onCreated}
    />
  );

  return (
    <section
      data-testid="now-panel"
      aria-labelledby={headingId}
      class="min-w-0 space-y-3 rounded-item p-4"
      style={{ 'background-color': 'var(--color-bg-panel)', 'box-shadow': 'var(--shadow-sm)' }}
    >
      <h2 id={headingId} class="text-xs font-semibold" style={{ color: 'var(--color-text-tertiary)' }}>Now</h2>

      <Show when={!latest()}>
        <div data-testid="now-idle" class="space-y-1.5">
          <p class="text-sm" style={muted()}>Nothing has run on this task yet.</p>
          <div class="flex flex-wrap items-center gap-2">
            <RunWithAgentButton
              itemId={props.itemId}
              itemTitle={props.itemTitle ?? ''}
              projectId={props.projectId ?? ''}
              testId="now-primary"
              onCreated={props.onCreated}
            />
          </div>
          {props.idle}
        </div>
      </Show>

      <Show when={latest()}>
        {(run) => (
          <div data-testid="latest-run" data-state={kind()} class="min-w-0 space-y-2">
            <div role="status" aria-live="polite">
              <RunState size="banner" {...runState()} />
            </div>
            <p data-testid="latest-run-status" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
              {statusLine()}
            </p>

            {/* A question waiting for the person comes first, with its answer open. On the Execution
                tab the attempt shows it, so it is asked for in one place. */}
            <Show when={asking() && !props.reviewInTab && attempt()}>
              {(a) => (
                <DecisionInbox
                  requestId={a().request_id}
                  attemptNumber={a().attempt_number}
                  attemptId={a().attempt_id}
                  attemptState={a().state}
                  pendingOnly
                  onResolved={reload}
                />
              )}
            </Show>

            <Show when={attempts().status === 'error'}>
              {(() => {
                const a = attempts();
                return a.status === 'error' ? <LoadError what="the run's attempts" error={a.error} onRetry={reload} /> : null;
              })()}
            </Show>

            <Show when={kind() === 'queued'}>
              <p data-testid="latest-run-waiting" class="text-sm" style={muted()}>
                <Show when={waitingReason()} fallback="Waiting for a runner to pick it up.">
                  Waiting:{' '}
                  <For each={waitingParts()}>
                    {(part) => (
                      <Show when={part.href} fallback={part.text}>
                        <a href={part.href} class="font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }}>
                          {part.text}
                        </a>
                      </Show>
                    )}
                  </For>
                </Show>
                <Show when={waitingReason() && latest()?.waiting_since}>
                  {(since) => <span style={{ color: 'var(--color-text-tertiary)' }}> · first seen {relativeTimeFromIso(since())}</span>}
                </Show>
              </p>
            </Show>

            <Show when={kind() === 'running' && eventsFailed()}>
              <LoadError what="the agent's last steps" error={events.error} onRetry={() => void refetch()} />
            </Show>

            <Show when={kind() === 'running'}>
              <TeamStepsLine profileId={runWho(latest()).profileId} agentKind={runWho(latest()).agentKind} events={events.state === 'errored' ? [] : (events.latest ?? [])} />
              <Show
                when={steps().length > 0}
                fallback={
                  <p class="text-sm" style={muted()}>
                    {asking()
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
                  <p data-testid={whyStopped() ? 'latest-run-why' : 'latest-run-failure'} class="text-sm" style={{ color: needsAPerson(attempt()) ? 'var(--color-text-primary)' : 'var(--color-danger-600)' }}>
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
              <Show when={folder()}>
                <p data-testid="latest-run-folder" class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>This team works in a folder: nothing to merge</p>
              </Show>
              <Show when={said()}>
                {(text) => (
                  <>
                    <Show when={noChange()}>
                      <p class="text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>{folder() ? 'What it did' : 'What it delivered'}</p>
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
            <Show when={questionCount() > 0}>
              <p data-testid="latest-run-questions" class="text-xs">
                <button type="button" class="font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }} onClick={() => props.onOpen()}>
                  The agent asked {questionCount()} {questionCount() === 1 ? 'question' : 'questions'}
                </button>
              </p>
            </Show>
            <Show when={latest()?.limit}>
              {(limit) => (
                <>
                  <Show when={limit().tokens !== null && limit().used !== null && limit().used !== undefined}>
                    <UsageMeter testId="run-limit-used" used={limit().used!} limit={limit().tokens!} />
                  </Show>
                  <RunLimitNote
                    limit={limit()}
                    running={false}
                    stopped={kind() === 'failed' && stoppedByLimit(attempt()?.terminal_reason)}
                    testId="latest-run-limit"
                    itemId={props.itemId}
                    itemTitle={props.itemTitle}
                    projectId={props.projectId}
                    onCreated={props.onCreated}
                  />
                </>
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
              <Show when={primary() === 'answer' && kind() === 'running'}>{openPrimary('Answer the question')}</Show>
              <Show when={primary() === 'answer' && kind() === 'queued' && askingBehind()}>
                {(() => {
                  const b = askingBehind()!;
                  return (
                    <Button
                      size="md"
                      data-testid="now-primary"
                      title={b.question ?? undefined}
                      onClick={() => window.location.assign(itemHref(b, `?tab=execution&run=${encodeURIComponent(b.request_id)}`))}
                    >
                      Answer it
                    </Button>
                  );
                })()}
              </Show>
              <Show when={primary() === 'review'}>{openPrimary('Review and merge')}</Show>
              <Show when={primary() === 'why'}>{openPrimary(needsPerson() ? 'Open the question' : 'See why it failed')}</Show>
              <Show when={primary() === 'again'}>{runAgain('primary')}</Show>

              <Show when={kind() === 'running'}>
                <Show when={!asking()}>{open('Follow live')}</Show>
              </Show>
              {/* One word, one place, for stopping a run that is alive: Stop. */}
              <Show when={kind() === 'running' || kind() === 'queued'}>
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
              <Show when={!props.reviewInTab && attempt() && ((kind() === 'review' && inlineReview()) || (kind() === 'done' && !!review()))}>
                <ReviewActions
                  requestId={attempt()!.request_id}
                  attemptNumber={attempt()!.attempt_number}
                  review={review()}
                  acceptVariant="secondary"
                  onReviewed={(r) => {
                    setSaved(r);
                    reload();
                  }}
                />
              </Show>
              <Show when={kind() === 'merged' || kind() === 'done'}>{open('See what it did')}</Show>
              {/* Another run is possible from every state that is not in flight, drawn as the quietest
                  button when something else is what to do now. A queued or running run (or one waiting
                  for an answer) is the run; a second one beside it would only compete. */}
              <Show when={primary() !== 'again' && kind() !== 'queued' && kind() !== 'running'}>{runAgain('ghost')}</Show>
            </div>

            <Show when={finished() && !noChange() && kind() !== 'failed' && !isMerged() && attempt()?.merge}>
              {(merge) => <MergeButton attemptId={attempt()!.attempt_id} merge={merge()} variant="secondary" onMerged={reload} />}
            </Show>
          </div>
        )}
      </Show>

      {props.footer}
    </section>
  );
};

export default LatestRunSummary;
