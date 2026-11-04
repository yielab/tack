import { type Component, Show, createEffect, createMemo, createSignal, on } from 'solid-js';
import { Badge, Button, EmptyState, Field, Icons, LoadError, RunState } from '../ui';
import { RUN_STATE_LABEL } from '../ui/RunState';
import { toast } from '../ui/toast';
import { useExecutionStore } from '../state/executionContext';
import AttemptList, { attemptRunState } from './AttemptList';
import RunChips, { chipState } from './RunChips';
import RunWithAgentModal from './RunWithAgentModal';
import { describeAttemptStatus, neverStarted } from './attemptFormat';
import { runWho } from './TeamSteps';
import { describeExecutionState, isTerminalStateString, relativeTimeFromIso } from './shared';
import type { AttemptSummary, ExecutionRequestRecord } from '../execution';
import RerunAtL3, { needsAPerson } from './RerunAtL3';
import RunLimitNote from './RunLimitNote';
import ContinueOrStartOver from './ContinueOrStartOver';
import { hasBudgetStopOffer, stoppedByLimit } from './runLimit';

export interface ExecutionTimelineProps {
  itemId: string;
  /** With the task's title and project, a run its limit stopped offers "Run again with more". */
  itemTitle?: string;
  projectId?: string;
  /** `page`: two columns, with what the agent got, its usage and the ids in the side one.
   *  `drawer` (the default): one column, in that same order. */
  layout?: 'drawer' | 'page';
  /** The run to show (its request id), as the address names it; the latest when absent. */
  runId?: string;
  /** Called with the request id of the run a chip picks or a new run starts, so the address can name it. */
  onRunChange?: (requestId: string | undefined) => void;
  /** The item's Now panel is on screen above this tab. It says the latest run's state, limit and token
   *  meter, so that run's section leaves them out and keeps only what Now does not say. */
  nowShown?: boolean;
}

/**
 * The request/attempt timeline plus cancel/reconcile controls. Reads
 * exclusively through the shared `useExecutionStore()` — no second fetch,
 * no local cache — so this component's view of a request is always the
 * same one the "Run with agent" modal and every other mounted consumer see.
 *
 * Renders every gap in the underlying data HONESTLY rather than papering
 * over it: `attemptsFor()` is an explicit state machine (idle / loading /
 * ready / error — see `store.ts`'s own header comment), so a genuinely
 * empty attempt list is never conflated with "still loading" or "the fetch
 * failed". `GET /executions/{id}/attempts` is wired in, so this component
 * shows real attempt data rather than a typed `not_available` placeholder.
 */
const ExecutionTimeline: Component<ExecutionTimelineProps> = (props) => {
  const store = useExecutionStore();
  const requests = createMemo(() => store.requestsForItem(props.itemId));

  // This tab needs the item's FULL request history, not just its most
  // recent one (what a Board badge's batched `watchItem` fetch carries) —
  // an item-scoped fetch, re-run whenever `props.itemId` changes,
  // guarantees that regardless of what else the install has recorded.
  createEffect(() => {
    void store.loadList(props.itemId);
  });

  // The run the tab shows: the latest until a chip picks another.
  const [pickedId, setPickedId] = createSignal<string | undefined>(props.runId);
  createEffect(on(() => props.runId, (id) => setPickedId(id), { defer: true }));
  const pick = (id: string | undefined) => {
    setPickedId(id);
    props.onRunChange?.(id);
  };
  const numbered = createMemo(() =>
    requests().map((record, i) => {
      const number = requests().length - i;
      return { record, number, id: record.summary?.request_id ?? `run-${number}` };
    }),
  );
  const active = createMemo(() => numbered().find((r) => r.id === pickedId()) ?? numbered()[0]);
  const chips = createMemo(() =>
    numbered().map(({ record, number, id }) => {
      const status = runStatus(store, record);
      return { id, label: `Run ${number}`, tone: status.tone, state: status.label };
    }),
  );
  const page = () => props.layout === 'page';
  const [side, setSide] = createSignal<HTMLElement | undefined>(undefined);

  return (
    <div class="space-y-3">
      <Show when={store.listStatus() === 'loading' && requests().length === 0}>
        <p class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading execution requests…
        </p>
      </Show>

      <Show when={store.listStatus() === 'error'}>
        <LoadError what="the runs" error={store.listError()} onRetry={() => void store.loadList(props.itemId)} />
      </Show>

      <Show
        when={requests().length > 0}
        fallback={
          <Show when={store.listStatus() !== 'loading'}>
            <EmptyState
              icon={<Icons.IconAgent size={28} />}
              title="No execution requests yet"
              description="Use “Run with agent” to start one."
            />
          </Show>
        }
      >
        <div data-testid="execution-columns" class={page() ? 'grid items-start gap-5 xl:grid-cols-[minmax(0,1fr)_18rem]' : 'space-y-3'}>
          <div class="min-w-0 space-y-3">
            {/* Newest first, so the first chip is the latest run; each carries its run's state. */}
            <Show when={requests().length > 1}>
              <RunChips chips={chips()} active={active()?.id ?? ''} onPick={pick} label="Runs" testId="run-chips" />
            </Show>
            <Show when={active()?.id} keyed>
              {(_id) => (
                <Show when={active()}>
                  {(a) => (
                    <RequestRow
                      record={a().record}
                      itemId={props.itemId}
                      itemTitle={props.itemTitle}
                      projectId={props.projectId}
                      number={a().number}
                      nowShown={props.nowShown && a().number === numbered()[0]?.number}
                      layout={props.layout}
                      sideTarget={side()}
                      onCreated={pick}
                    />
                  )}
                </Show>
              )}
            </Show>
          </div>
          <Show when={page()}>
            <div ref={setSide} data-testid="execution-side" class="min-w-0 space-y-4" />
          </Show>
        </div>
      </Show>
    </div>
  );
};

/** The one status a run shows: its latest attempt's, which knows more (Running, needs your
 *  review, Accepted) than the request's own state, which stays "leased" for as long as an
 *  attempt is out. */
function runStatus(store: ReturnType<typeof useExecutionStore>, record: ExecutionRequestRecord) {
  const a = store.attemptsFor(record.summary?.request_id ?? '');
  const latest = a.status === 'ready' ? latestOf(a.data) : undefined;
  if (latest) return { ...describeAttemptStatus(latest, latest.review), known: true };
  // Attempts not loaded yet: a run turned away before it started still reads Not started, as the board's card does.
  const s = record.summary;
  if (s && neverStarted(s)) return { ...describeAttemptStatus({ state: s.state, terminal_reason: null, never_started: true }), known: true };
  return describeExecutionState(s?.state ?? 'unknown');
}

const latestOf = (attempts: AttemptSummary[]) =>
  attempts.reduce<AttemptSummary | undefined>((max, x) => (!max || x.attempt_number > max.attempt_number ? x : max), undefined);

const RequestRow: Component<{
  record: ExecutionRequestRecord;
  itemId: string;
  itemTitle?: string;
  projectId?: string;
  number: number;
  /** Now says this run's state and limit: this section does not say them again. */
  nowShown?: boolean;
  layout?: 'drawer' | 'page';
  sideTarget?: HTMLElement;
  onCreated: (requestId: string) => void;
}> = (props) => {
  const store = useExecutionStore();
  const [reconcileOpen, setReconcileOpen] = createSignal(false);
  const [recoveryKey, setRecoveryKey] = createSignal('');
  const [reason, setReason] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const summary = () => props.record.summary;
  const state = () => summary()?.state ?? 'unknown';

  const canCancel = createMemo(() => {
    const s = summary();
    if (!s) return false;
    return !isTerminalStateString(s.state) && !props.record.cancellation.pending && !props.record.cancellation.requested;
  });

  const canReconcile = createMemo(() => summary()?.state === 'needs_operator');

  const cancel = async () => {
    const s = summary();
    if (!s) return;
    setBusy(true);
    try {
      await store.cancel(s.request_id);
      toast.success('Stop requested.');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to request cancellation.');
    } finally {
      setBusy(false);
    }
  };

  // An ambiguous (needs_operator) state requires explicit operator action,
  // never an automatic silent retry: the recovery key and reason are always
  // typed in by hand, never pre-filled or defaulted — there is no
  // "reconcile" button that fires with zero arguments.
  const reconcile = async (e: Event) => {
    e.preventDefault();
    const s = summary();
    if (!s || !recoveryKey().trim() || !reason().trim()) return;
    setBusy(true);
    try {
      await store.requeue(s.request_id, { recovery_key: recoveryKey().trim(), reason: reason().trim() });
      toast.success('Execution requeued.');
      setReconcileOpen(false);
      setRecoveryKey('');
      setReason('');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to requeue.');
    } finally {
      setBusy(false);
    }
  };

  const attempts = createMemo(() => store.attemptsFor(summary()?.request_id ?? ''));
  const readyAttempts = () => {
    const a = attempts();
    return a.status === 'ready' ? a.data : [];
  };

  const latestAttempt = () => latestOf(readyAttempts());

  const status = createMemo(() => runStatus(store, props.record));
  // The run in the one language every screen uses: its latest attempt's state, else the request's own.
  const banner = () => {
    const a = latestAttempt();
    return a ? attemptRunState(a) : { state: chipState(status().tone, status().label) };
  };

  // Starting another run of the task: the verdict offers it as its action once nothing else is.
  const [again, setAgain] = createSignal(false);
  const canRerun = () => !!props.itemTitle && !!props.projectId;
  // Another run is offered only when none of the task's runs is alive (queued, running or waiting for an answer).
  const taskHasLiveRun = () =>
    store.requestsForItem(props.itemId).some((r) => r.status === 'ready' && r.summary !== undefined && !isTerminalStateString(r.summary.state));
  const runAgain = (variant: 'primary' | 'secondary') => (
    <Show when={!taskHasLiveRun()}>
      <Button
        size="sm"
        variant={variant}
        disabled={!canRerun()}
        title={canRerun() ? undefined : 'This screen does not know the task'}
        onClick={() => setAgain(true)}
      >
        Run again
      </Button>
      <Show when={canRerun()}>
        <RunWithAgentModal
          isOpen={again()}
          onClose={() => setAgain(false)}
          itemId={props.itemId}
          itemTitle={props.itemTitle!}
          projectId={props.projectId!}
          onCreated={props.onCreated}
        />
      </Show>
    </Show>
  );

  // Lazy first load: fetch attempts once per request the moment its row is
  // rendered, never repeatedly — a subsequent render with the same `idle`
  // state (there isn't one, since `loadAttempts` always transitions past
  // `idle`) would otherwise re-trigger. Realtime refresh is handled by
  // `store.ts#connectRealtime` for any request this effect has already
  // asked about.
  createEffect(() => {
    const id = summary()?.request_id;
    if (id && attempts().status === 'idle') void store.loadAttempts(id);
  });

  return (
    <section data-testid="run" class="min-w-0 space-y-4 border-t pt-5" style={{ 'border-color': 'var(--color-border-light)' }}>
      <div class="flex flex-wrap items-center gap-2">
        <h3 class="font-heading text-lg" style={{ color: 'var(--color-text-primary)' }}>Run {props.number}</h3>
        <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          created {relativeTimeFromIso(summary()?.created_at)}
        </span>
        {/* Once there is an attempt the ids sit with it; before that, this is the only place. */}
        <Show when={readyAttempts().length === 0}>
          <span class="ml-auto max-w-[14rem] truncate text-micro" title={summary()?.request_id} style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
            {summary()?.request_id ?? '—'}
          </span>
        </Show>
        {/* Only while it is true: once the run has ended its state says how. */}
        <Show when={(props.record.cancellation.requested || props.record.cancellation.pending) && !isTerminalStateString(state())}>
          <Badge tone="warning">{props.record.cancellation.pending ? 'Cancellation pending' : 'Cancellation requested'}</Badge>
        </Show>
        <Show when={props.record.cancellation.conflict}>
          <Badge tone="danger">Already terminal — cancel had no effect</Badge>
        </Show>
      </div>

      {/* The run's one state, announced politely as it changes; the attempt's own words come under it
          when the banner's say less. */}
      <Show when={!props.nowShown || !status().known}>
      <div role="status" aria-live="polite" class="min-w-0 space-y-1">
        <Show
          when={status().known}
          fallback={
            <div class="flex flex-wrap items-center gap-2">
              <Badge tone={status().tone}>{status().label}</Badge>
              <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                (unrecognised state — showing raw value)
              </span>
            </div>
          }
        >
          <RunState size="banner" {...banner()} />
          <Show when={status().label !== RUN_STATE_LABEL[banner().state]}>
            <p data-testid="run-status-words" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>{status().label}</p>
          </Show>
        </Show>
      </div>
      </Show>

      <Show when={!props.nowShown && state() === 'queued' && summary()?.waiting_reason}>
        {(why) => (
          <p data-testid="run-waiting" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            Waiting: {why()}
            <Show when={summary()?.waiting_since}>
              {(since) => <span style={{ color: 'var(--color-text-tertiary)' }}> · first seen {relativeTimeFromIso(since())}</span>}
            </Show>
          </p>
        )}
      </Show>
      <Show when={!props.nowShown && state() === 'failed' && summary()?.waiting_reason}>
        {(why) => (
          <p data-testid="run-failure-reason" class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
            {why()}
          </p>
        )}
      </Show>

      <Show when={!props.nowShown && summary()?.limit}>
        {(limit) => (
          <RunLimitNote
            limit={limit()}
            running={!isTerminalStateString(state())}
            stopped={
              state() === 'failed' &&
              stoppedByLimit(latestAttempt()?.terminal_reason) &&
              !hasBudgetStopOffer(latestAttempt()?.terminal_reason)
            }
            testId="run-limit"
            itemId={props.itemId}
            itemTitle={props.itemTitle}
            projectId={props.projectId}
          />
        )}
      </Show>

      <Show when={!props.nowShown && latestAttempt()}>
        {(attempt) => (
          <ContinueOrStartOver
            attempt={attempt()}
            tokenLimit={summary()?.limit?.tokens}
            itemId={props.itemId}
            itemTitle={props.itemTitle}
            projectId={props.projectId}
            onCreated={props.onCreated}
          />
        )}
      </Show>

      <Show when={!props.nowShown && needsAPerson(latestAttempt())}>
        <RerunAtL3 attempt={latestAttempt()!} itemId={props.itemId} itemTitle={props.itemTitle} projectId={props.projectId} onCreated={props.onCreated} />
      </Show>

      <Show when={props.record.error}>
        {(err) => (
          <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
            {err().message}
          </p>
        )}
      </Show>

      {/* Attempt timeline — real data, from a wired endpoint.
          Every fetch state is rendered explicitly. */}
      <Show when={attempts().status === 'loading'}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading attempts…
        </p>
      </Show>
      <Show when={attempts().status === 'error'}>
        {(() => {
          const a = attempts();
          return a.status === 'error' ? (
            <LoadError what="the attempts" error={a.error} onRetry={() => void store.loadAttempts(summary()?.request_id ?? '').catch(() => undefined)} />
          ) : null;
        })()}
      </Show>
      {/* Mounted once and fed the fresh attempts, so a refresh updates the
          rows in place and what the person opened on them stays open. */}
      <Show when={attempts().status === 'ready'}>
        <Show
          when={readyAttempts().length > 0}
          fallback={
            <>
              <p class="rounded-item border-2 border-dashed px-4 py-3 text-xs" style={{ color: 'var(--color-text-tertiary)', 'border-color': 'var(--color-border-light)' }}>
                No attempts yet.
              </p>
              <div class="flex flex-wrap items-center gap-2">{runAgain('secondary')}</div>
            </>
          }
        >
          <AttemptList
            requestId={summary()?.request_id ?? ''}
            attempts={readyAttempts()}
            itemId={props.itemId}
            limit={summary()?.limit}
            profileId={runWho(summary()).profileId}
            agentKind={runWho(summary()).agentKind}
            runAgain={runAgain}
            layout={props.layout}
            sideTarget={props.sideTarget}
            nowShown={props.nowShown}
            onReviewed={() => {
              const id = summary()?.request_id;
              if (id) void store.loadAttempts(id).catch(() => undefined);
            }}
          />
        </Show>
      </Show>

      <div class="flex flex-wrap items-center gap-2">
        {/* The Now panel above already has Stop for the newest run: the same button twice would only compete. */}
        <Show when={canCancel() && !props.nowShown}>
          <Button size="sm" variant="secondary" onClick={cancel} disabled={busy()} loading={busy()}>
            Stop
          </Button>
        </Show>
        <Show when={canReconcile()}>
          <Button size="sm" variant="secondary" onClick={() => setReconcileOpen((v) => !v)} disabled={busy()}>
            Reconcile…
          </Button>
        </Show>
      </div>

      <Show when={reconcileOpen()}>
        <form
          class="space-y-3 rounded-card p-5"
          style={{ 'background-color': 'var(--color-bg-app)', 'box-shadow': 'var(--shadow-sm)' }}
          onSubmit={reconcile}
        >
          <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            This request needs an operator's explicit decision before it can requeue — enter the recovery key and a
            reason (audited).
          </p>
          <Field
            label="Recovery key"
            value={recoveryKey()}
            onInput={(e) => setRecoveryKey(e.currentTarget.value)}
            required
          />
          <Field label="Reason" value={reason()} onInput={(e) => setReason(e.currentTarget.value)} required />
          <div class="flex justify-end gap-2 pt-1">
            <Button type="button" size="sm" variant="secondary" onClick={() => setReconcileOpen(false)} disabled={busy()}>
              Cancel
            </Button>
            <Button type="submit" size="sm" disabled={busy() || !recoveryKey().trim() || !reason().trim()} loading={busy()}>
              Confirm requeue
            </Button>
          </div>
        </form>
      </Show>
    </section>
  );
};

export default ExecutionTimeline;
