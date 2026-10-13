import { type Component, For, Show, createMemo, createResource, createSignal } from 'solid-js';
import { Badge, Button, Modal } from '../ui';
import { toast } from '../ui/toast';
import { api } from '../api';
import { artifactsApi, attemptsApi, type AttemptSummary } from '../execution';
import { ITEM_UPDATED_EVENT } from '../state/itemEvents';
import {
  NOT_MEASURED_TEXT,
  describeAttemptOutcome,
  describeAttemptStatus,
  describeModelProvenance,
  formatApproxCost,
  formatTokenCount,
  formatWallClock,
  harnessName,
  isReviewableAttempt,
  isTerminalAttemptState,
  readAttemptUsage,
} from './attemptFormat';
import ArtifactDownloadPanel, { ArtifactView } from './ArtifactDownloadPanel';
import { invoke } from '@tauri-apps/api/core';
import { request } from '../api/client';
import { SANDBOX_TEXT } from '../help/texts';
import { inDesktop } from './desktop';
import { describeStep, parseRunLog } from './runLog';
import DiffView from './DiffView';
import DecisionInbox from './DecisionInbox';
import EventTimeline from './EventTimeline';
import MrpPanel from './MrpPanel';
import PlanPanel from './PlanPanel';
import { describeExecutionState, describePullRequestState, relativeTimeFromIso } from './shared';

export interface AttemptListProps {
  requestId: string;
  attempts: AttemptSummary[];
  /** The item the request belongs to; the plan panel creates subtasks under it. */
  itemId?: string;
  /** Called once a review verdict is saved, so the host can refresh the run's status. */
  onReviewed?: () => void;
}

/** What the agent did (its steps, from a claude-code log) and what it said at the end. */
const AgentReport: Component<{ steps: ReturnType<typeof parseRunLog> | undefined; result: string | null }> = (props) => {
  const [all, setAll] = createSignal(false);
  const steps = () => props.steps?.steps ?? [];
  const shown = () => (all() ? steps() : steps().slice(0, 8));
  const noTools = () => props.steps?.toolsOffered?.length === 0;
  return (
    <div class="space-y-3">
      <Show when={noTools()}>
        <p class="rounded-[20px] px-4 py-3 text-sm" style={{ 'background-color': 'var(--color-warning-100)', color: 'var(--color-warning-700)' }}>
          The agent had no tools in this run, so it could not read or change files.
        </p>
      </Show>
      <Show when={steps().length > 0}>
        <section>
          <h4 class="mb-1.5 text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>What the agent did</h4>
          <ol data-testid="agent-steps" class="space-y-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            <For each={shown()}>
              {(step) => (
                <li class="flex gap-2 break-all">
                  <span aria-hidden="true" style={{ color: step.failed ? 'var(--color-danger-600)' : 'var(--color-text-tertiary)' }}>{step.failed ? '✕' : '•'}</span>
                  <span>{describeStep(step)}{step.failed ? ' (failed)' : ''}</span>
                </li>
              )}
            </For>
          </ol>
          <Show when={steps().length > 8}>
            <button type="button" class="mt-1 text-xs font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }} onClick={() => setAll((v) => !v)}>
              {all() ? 'Show fewer' : `Show all ${steps().length} steps`}
            </button>
          </Show>
        </section>
      </Show>
      <Show when={props.result}>
        {(r) => (
          <section>
            <h4 class="mb-1.5 text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>What the agent said</h4>
            <p data-testid="agent-result" class="max-h-60 overflow-auto whitespace-pre-wrap rounded-[20px] px-4 py-3 text-sm"
              style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)' }}>{r()}</p>
          </section>
        )}
      </Show>
    </div>
  );
};

/** The pack panel, shown only when the attempt's artifacts include a
 *  merge-readiness pack (kind `mrp`). */
const MrpSlot: Component<{ requestId: string; attemptNumber: number }> = (props) => {
  const [artifacts] = createResource(
    () => `${props.requestId}:${props.attemptNumber}`,
    () => artifactsApi.list(props.requestId, props.attemptNumber).catch(() => []),
  );
  return (
    <Show when={(artifacts() ?? []).some((a) => a.kind === 'mrp')}>
      <section>
        <h4 class="mb-2 text-base" style={{ color: 'var(--color-text-primary)' }}>
          Merge-readiness pack
        </h4>
        <MrpPanel requestId={props.requestId} attemptNumber={props.attemptNumber} />
      </section>
    </Show>
  );
};

/** What the agent was given: its sandbox, the names of its environment variables, where it
 *  worked, and (collapsed) the exact prompt it read. */
const AgentGot: Component<{ requestId: string; attempt: AttemptSummary; workspace: string | null }> = (props) => {
  const [opened, setOpened] = createSignal(false);
  const [instructions] = createResource(
    () => (opened() ? props.requestId : undefined),
    async (id) => {
      try {
        return (await request<{ instructions?: string | null }>(`/executions/${encodeURIComponent(id)}`)).instructions ?? null;
      } catch {
        return null;
      }
    },
  );
  const sandbox = () => (props.attempt.sandbox === 'bwrap' || props.attempt.sandbox === 'none' ? SANDBOX_TEXT[props.attempt.sandbox] : null);
  const names = () => props.attempt.environment_names ?? [];
  return (
    <section data-testid="agent-got" class="space-y-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
      <h4 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>What the agent got</h4>
      <p data-testid="agent-got-sandbox">{sandbox() ?? 'The runner did not say whether the agent was sandboxed'}</p>
      <p data-testid="agent-got-env" class="break-all">
        Environment variables: {names().length > 0 ? names().join(', ') : 'none reported'}
      </p>
      <Show when={props.workspace}>
        {(w) => <p data-testid="agent-got-workspace" class="break-all">Workspace: {w()}</p>}
      </Show>
      <details onToggle={(e) => setOpened(e.currentTarget.open)}>
        <summary class="cursor-pointer text-xs font-semibold" style={{ color: 'var(--color-accent-ink)' }}>What the agent read</summary>
        <Show when={instructions.loading}><p class="text-xs">Loading the prompt…</p></Show>
        <Show when={instructions()}>
          {(text) => (
            <pre data-testid="agent-read" class="mt-1 max-h-72 overflow-auto whitespace-pre-wrap rounded-[20px] px-4 py-3 text-xs"
              style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)' }}>{text()}</pre>
          )}
        </Show>
      </details>
    </section>
  );
};

/** The row of actions on a terminal attempt: see what it did. */
const AttemptTools: Component<{ requestId: string; attempt: AttemptSummary; itemId?: string }> = (props) => {
  const n = () => props.attempt.attempt_number;
  const reason = () => (props.attempt.terminal_reason ?? {}) as { workspace_kept_at?: unknown; result?: unknown };
  const [artifacts] = createResource(
    () => `${props.requestId}:${n()}`,
    () => artifactsApi.list(props.requestId, n()).catch(() => []),
  );
  const patch = () => (artifacts() ?? []).find((a) => a.kind === 'patch');
  const log = () => (artifacts() ?? []).find((a) => a.kind === 'log' || (a.name ?? '').endsWith('.log'));
  const plan = () => (artifacts() ?? []).find((a) => a.kind === 'plan');
  const evidence = () => (artifacts() ?? []).find((a) => a.kind === 'evidence');
  const [branch] = createResource(evidence, async (e) => {
    try {
      const parsed = JSON.parse(await (await artifactsApi.download(props.requestId, n(), e.artifact_id)).text()) as {
        branch?: unknown;
      };
      const b = parsed.branch;
      if (typeof b === 'string') return b;
      const inner = (b as { branch?: unknown } | null)?.branch;
      return typeof inner === 'string' ? inner : null;
    } catch {
      return null;
    }
  });
  // The project's repository: where a run in the user's own folder leaves its change, and the
  // root the desktop app checks a folder against before it opens one.
  const [project] = createResource(
    () => (patch() ? props.itemId : undefined),
    async (id) => {
      try {
        const p = await api.projects.get((await api.items.get(id)).project_id);
        return {
          repository: p.repository ?? null,
          own: p.workspace_mode === 'local_branch' || p.workspace_mode === 'in_place',
        };
      } catch {
        return null;
      }
    },
  );
  const repository = () => project()?.repository ?? null;
  const folder = () =>
    typeof reason().workspace_kept_at === 'string'
      ? (reason().workspace_kept_at as string)
      : project()?.own
        ? repository()
        : null;
  const result = () => (typeof reason().result === 'string' ? (reason().result as string) : null);
  const [steps] = createResource(log, async (l) => {
    try {
      return parseRunLog(await (await artifactsApi.download(props.requestId, n(), l.artifact_id)).text());
    } catch {
      return undefined;
    }
  });
  const [diffOpen, setDiffOpen] = createSignal(false);
  const [diffText] = createResource(
    () => (diffOpen() ? patch() : undefined),
    async (p) => (await artifactsApi.download(props.requestId, n(), p.artifact_id)).text(),
  );
  const [logOpen, setLogOpen] = createSignal(false);
  const copy = async (text: string, what: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(`${what} copied.`);
    } catch {
      toast.error(`Couldn't copy the ${what.toLowerCase()}: ${text}`);
    }
  };
  const canOpen = () => inDesktop() && repository() !== null;
  const openFolder = async (path: string) => {
    if (!canOpen()) return copy(path, 'Path');
    try {
      await invoke('open_folder', { path, repository: repository() });
    } catch (err) {
      toast.error(`Couldn't open ${path}: ${err instanceof Error ? err.message : String(err)}`);
    }
  };
  const patchHasChanges = () => (patch()?.size_bytes ?? 0) > 0;
  return (
    <Show when={isTerminalAttemptState(props.attempt.state)}>
      <div class="space-y-2">
        <div data-testid="attempt-tools" class="flex flex-wrap items-center gap-2">
          <Show when={patchHasChanges()}>
            <Button size="sm" variant="secondary" onClick={() => setDiffOpen(true)}>Open diff</Button>
          </Show>
          <Show when={patch() && folder()}>
            {(path) => (
              <Button size="sm" variant="secondary" title={path()} onClick={() => void openFolder(path())}>
                {canOpen() ? 'Open folder' : 'Copy path'}
              </Button>
            )}
          </Show>
          <Show when={branch()}>
            {(b) => (
              <Button size="sm" variant="secondary" onClick={() => void copy(b(), 'Branch')}>Copy branch</Button>
            )}
          </Show>
          <Show when={log()}>
            <Button size="sm" variant="secondary" onClick={() => setLogOpen(true)}>Log</Button>
          </Show>
        </div>
        <AgentGot requestId={props.requestId} attempt={props.attempt} workspace={folder()} />
        <AgentReport steps={steps()} result={result()} />
        <ArtifactView requestId={props.requestId} attemptNumber={n()} artifact={logOpen() ? log() : undefined} onClose={() => setLogOpen(false)} />
        <Show when={plan() && props.itemId}>
          {(itemId) => (
            <PlanPanel requestId={props.requestId} attemptNumber={n()} artifactId={plan()!.artifact_id} itemId={itemId()} />
          )}
        </Show>
        <Modal isOpen={diffOpen()} onClose={() => setDiffOpen(false)} title="Changes" size="xl">
          <Show when={diffText()} fallback={<p class="text-sm">Loading the diff…</p>}>
            {(t) => <DiffView patch={t()} />}
          </Show>
        </Modal>
      </div>
    </Show>
  );
};

const AttemptRow: Component<{
  requestId: string;
  attempt: AttemptSummary;
  itemId?: string;
  numbered: boolean;
  onReviewed?: () => void;
}> = (props) => {
  const [expanded, setExpanded] = createSignal(false);
  const stateInfo = () => describeExecutionState(props.attempt.state);
  const outcome = () => describeAttemptOutcome(props.attempt.state, props.attempt.terminal_reason);
  const harnessKind = () => (props.attempt.actual_execution as { harness_kind?: string } | null)?.harness_kind ?? null;
  // The verdict as last read from the server: the prop's until a review is
  // submitted here, then the refetched attempt's.
  const [fresh, setFresh] = createSignal<AttemptSummary['review'] | undefined>(undefined);
  const review = () => (fresh() !== undefined ? fresh() : props.attempt.review);
  const status = () => describeAttemptStatus(props.attempt, review());
  const awaitingReview = () => isReviewableAttempt(props.attempt) && !review();
  const terminal = () => isTerminalAttemptState(props.attempt.state);
  const [note, setNote] = createSignal('');
  const [reviewing, setReviewing] = createSignal(false);
  const [reviewError, setReviewError] = createSignal<string | null>(null);
  const submitReview = async (verdict: 'accepted' | 'rejected') => {
    setReviewing(true);
    setReviewError(null);
    try {
      const n = note().trim();
      await attemptsApi.review(props.requestId, props.attempt.attempt_number, { verdict, ...(n ? { note: n } : {}) });
      const list = await attemptsApi.list(props.requestId);
      const mine = list.data.data.find((a) => a.attempt_number === props.attempt.attempt_number);
      setFresh(mine?.review ?? null);
      props.onReviewed?.();
      window.dispatchEvent(new CustomEvent(ITEM_UPDATED_EVENT));
    } catch (e) {
      setReviewError(e instanceof Error ? e.message : 'The verdict could not be saved');
    } finally {
      setReviewing(false);
    }
  };

  return (
    <li data-testid="attempt" class="space-y-4 rounded-[20px] p-4" style={{ 'background-color': 'var(--color-bg-app)', 'box-shadow': 'var(--shadow-sm)' }}>
      <div class="flex flex-wrap items-center gap-2">
        {/* A run with one attempt shows its status in the run's own heading. */}
        <Show when={props.numbered}>
          <span class="font-heading text-lg" style={{ color: 'var(--color-text-primary)' }}>
            Attempt {props.attempt.attempt_number}
          </span>
          <Badge tone={status().tone}>{status().label}</Badge>
        </Show>
        <Show when={!stateInfo().known}>
          <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            (unrecognised state)
          </span>
        </Show>
        <Show when={props.attempt.pull_request}>
          {(pr) => {
            const prState = () => describePullRequestState(pr().state);
            return (
              <a
                href={pr().url}
                target="_blank"
                rel="noopener noreferrer"
                class="flex items-center gap-2"
              >
                <span class="text-xs" style={{ color: 'var(--color-primary-600)' }}>
                  PR #{pr().number}
                </span>
                <Badge tone={prState().tone}>{prState().label}</Badge>
              </a>
            );
          }}
        </Show>
        <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          <Show when={harnessKind()}>{(kind) => `${harnessName(kind())} · `}</Show>
          started {relativeTimeFromIso(props.attempt.started_at ?? props.attempt.lease_issued_at)}
        </span>
        <span class="ml-auto max-w-[14rem] truncate text-[11px]" title={`Runner ${props.attempt.runner_id}`} style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
          {props.attempt.runner_id}
        </span>
      </div>

      <Show when={outcome().whyStopped}>
        {(why) => (
          <p class="text-sm" style={{ color: 'var(--color-text-primary)' }}>
            <strong>Why it stopped:</strong> {why()}
          </p>
        )}
      </Show>
      <Show when={outcome().badge?.detail}>
        {(detail) => (
          <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            {detail()}
          </p>
        )}
      </Show>

      <Show when={review()}>
        {(r) => (
          <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            {r().verdict === 'accepted' ? 'Accepted' : 'Rejected'} {relativeTimeFromIso(r().reviewed_at)}
            {r().note ? ` — ${r().note}` : ''}
          </p>
        )}
      </Show>
      <Show when={awaitingReview()}>
        <div class="flex flex-wrap items-center gap-2">
          <input
            type="text"
            aria-label="Review note (optional)"
            placeholder="Note (optional)"
            value={note()}
            onInput={(e) => setNote(e.currentTarget.value)}
            class="min-w-[12rem] flex-1 rounded-full px-3 py-1.5 text-sm"
            style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)', border: 'none' }}
          />
          <Button size="sm" disabled={reviewing()} onClick={() => void submitReview('accepted')}>
            Accept
          </Button>
          <Button size="sm" variant="secondary" disabled={reviewing()} onClick={() => void submitReview('rejected')}>
            Reject
          </Button>
          <Show when={reviewError()}>
            <span class="text-xs" style={{ color: 'var(--color-danger-700)' }}>{reviewError()}</span>
          </Show>
        </div>
      </Show>

      <AttemptTools requestId={props.requestId} attempt={props.attempt} itemId={props.itemId} />

      <Show
        when={terminal()}
        fallback={
          <p data-testid="attempt-working" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            {props.attempt.state === 'waiting_decision'
              ? 'The agent is waiting for your answer — see Questions from the agent below.'
              : 'The agent is working. Its steps appear in the timeline as it takes them; the tokens it used and its cost appear here when it finishes.'}
          </p>
        }
      >
        <UsagePanel attempt={props.attempt} />
      </Show>

      <Button size="sm" variant="ghost" onClick={() => setExpanded((v) => !v)} aria-expanded={expanded()}>
        {expanded() ? 'Hide details' : 'Show timeline, questions & files'}
      </Button>

      <Show when={expanded()}>
        <div class="space-y-5 border-t pt-4" style={{ 'border-color': 'var(--color-border-light)' }}>
          <section>
            <h4 class="mb-2 text-base" style={{ color: 'var(--color-text-primary)' }}>
              Timeline
            </h4>
            <EventTimeline requestId={props.requestId} attemptNumber={props.attempt.attempt_number} live={!terminal()} />
          </section>

          <section>
            <h4 class="mb-2 text-base" style={{ color: 'var(--color-text-primary)' }}>
              Questions from the agent
            </h4>
            <DecisionInbox
              requestId={props.requestId}
              attemptNumber={props.attempt.attempt_number}
              attemptId={props.attempt.attempt_id}
            />
          </section>

          <section>
            <h4 class="mb-2 text-base" style={{ color: 'var(--color-text-primary)' }}>
              Files from this run
            </h4>
            <ArtifactDownloadPanel requestId={props.requestId} attemptNumber={props.attempt.attempt_number} />
          </section>

          <MrpSlot requestId={props.requestId} attemptNumber={props.attempt.attempt_number} />
        </div>
      </Show>
    </li>
  );
};

/** An unmeasured figure keeps its literal "Not measured" text and is set
 *  apart (italic, dashed underline) so it can never be read as a real amount. */
const Unmeasured: Component = () => (
  <span class="inline-block border-b border-dashed italic" style={{ color: 'var(--color-text-secondary)', 'border-color': 'var(--color-border-medium)' }}>
    {NOT_MEASURED_TEXT}
  </span>
);

/** What a finished attempt used, tokens first: they are what the harness
 *  measured; the dollar figure is its estimate, shown as approximate. The
 *  model provenance sits beside the model, and only when it says something
 *  the model name doesn't. */
const UsagePanel: Component<{ attempt: AttemptSummary }> = (props) => {
  const usage = () => readAttemptUsage(props.attempt);
  const harness = () => harnessName((props.attempt.actual_execution as { harness_kind?: string } | null)?.harness_kind);
  const provenance = () => describeModelProvenance(props.attempt.model_provenance);
  // Nothing named a model, so there is none to show.
  const modelUnknown = () => !props.attempt.model_provenance?.served;
  const exact = (n: number | null) => (n === null ? '' : n.toLocaleString('en-US'));
  const cacheLine = () => {
    const u = usage();
    if (u.cacheRead === null && u.cacheWrite === null) return null;
    const parts: string[] = [];
    if (u.cacheRead !== null) parts.push(`${formatTokenCount(u.cacheRead)} read from cache`);
    if (u.cacheWrite !== null) parts.push(`${formatTokenCount(u.cacheWrite)} written to cache`);
    if (u.tokensIn !== null) {
      const fresh = u.tokensIn - (u.cacheRead ?? 0) - (u.cacheWrite ?? 0);
      if (fresh >= 0) parts.push(`${formatTokenCount(fresh)} new`);
    }
    return `Of the input: ${parts.join(', ')}.`;
  };
  const facts = () => {
    const u = usage();
    const out: string[] = [];
    if (u.modelCalls !== null) out.push(`${u.modelCalls} model ${u.modelCalls === 1 ? 'call' : 'calls'}`);
    if (u.wallClockMs !== null) out.push(formatWallClock(u.wallClockMs, true));
    return out;
  };
  const cost = () => formatApproxCost(usage().cost);
  return (
    <section data-testid="attempt-usage" aria-label="Usage" class="space-y-2 rounded-[20px] px-4 py-3" style={{ 'background-color': 'var(--color-bg-panel)' }}>
      <div class="flex flex-wrap items-baseline gap-x-4 gap-y-1">
        <Show
          when={usage().tokensIn !== null || usage().tokensOut !== null}
          fallback={<span class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>Tokens: <Unmeasured /></span>}
        >
          <span data-testid="usage-tokens" class="text-base font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            <span title={`${exact(usage().tokensIn)} tokens in`}>{usage().tokensIn === null ? '?' : formatTokenCount(usage().tokensIn!)} tokens in</span>
            {' · '}
            <span title={`${exact(usage().tokensOut)} tokens out`}>{usage().tokensOut === null ? '?' : formatTokenCount(usage().tokensOut!)} out</span>
          </span>
        </Show>
        <Show
          when={cost() !== NOT_MEASURED_TEXT}
          fallback={<span data-testid="usage-cost" class="text-sm" title={`${harness()} did not report a cost`} style={{ color: 'var(--color-text-secondary)' }}>Cost: <Unmeasured /></span>}
        >
          <span data-testid="usage-cost" class="text-sm font-semibold" title={`${harness()}'s own estimate at list price — your bill may differ`} style={{ color: 'var(--color-text-primary)' }}>
            {cost()} (approx.)
          </span>
        </Show>
      </div>
      <Show when={cacheLine()}>
        {(line) => <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>{line()}</p>}
      </Show>
      <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
        <Show
          when={provenance().tone !== 'warning' && usage().models.length > 0}
          fallback={
            <Show when={props.attempt.model_provenance && !modelUnknown()} fallback={<span>Model not reported</span>}>
              <Show when={provenance().tone === 'warning'}>
                <Badge tone="warning">{provenance().label}</Badge>
              </Show>
              <span>{provenance().detail}</span>
            </Show>
          }
        >
          <span data-testid="usage-models" style={{ 'font-family': 'var(--font-mono)' }}>{usage().models.join(', ')}</span>
          <Show when={provenance().tone === 'info'}>
            <span title={provenance().detail}><Badge tone="info">{provenance().label}</Badge></span>
          </Show>
        </Show>
        <For each={facts()}>{(fact) => <span>· {fact}</span>}</For>
      </div>
      <Show when={cost() !== NOT_MEASURED_TEXT}>
        <p class="text-[11px]" style={{ color: 'var(--color-text-tertiary)' }}>
          The cost is {harness()}'s own estimate at list price, not your bill.
        </p>
      </Show>
    </section>
  );
};

/**
 * Every attempt made against one execution request, reading real data from
 * `store.ts#attemptsFor`/`loadAttempts`. Each row exposes model provenance,
 * usage economics, and (on demand, to avoid an eager fetch for every attempt
 * of every visible request) its normalized event timeline, decision inbox,
 * and artifact download action.
 */
const AttemptList: Component<AttemptListProps> = (props) => {
  // Rows follow the attempt's id, not its object: a refresh that brings the
  // same attempt changed (a new event moves its checkpoint) updates its row in
  // place, so what the person opened on it stays open.
  const ids = createMemo(() => props.attempts.map((attempt) => attempt.attempt_id));
  const byId = createMemo(() => new Map(props.attempts.map((attempt) => [attempt.attempt_id, attempt])));
  return (
    <ul class="space-y-2">
      <For each={ids()}>
        {(id) => (
          <Show when={byId().get(id)}>
            {(attempt) => (
              <AttemptRow
                requestId={props.requestId}
                attempt={attempt()}
                itemId={props.itemId}
                numbered={props.attempts.length > 1}
                onReviewed={props.onReviewed}
              />
            )}
          </Show>
        )}
      </For>
    </ul>
  );
};

export default AttemptList;
