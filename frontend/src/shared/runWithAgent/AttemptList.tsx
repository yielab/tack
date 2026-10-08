import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button, Modal } from '../ui';
import { toast } from '../ui/toast';
import { api } from '../api';
import { artifactsApi, attemptsApi, type AttemptSummary } from '../execution';
import { ITEM_UPDATED_EVENT } from '../state/itemEvents';
import {
  NOT_MEASURED_TEXT,
  describeAttemptOutcome,
  describeModelProvenance,
  formatUsageEconomics,
  isTerminalAttemptState,
} from './attemptFormat';
import ArtifactDownloadPanel, { ArtifactView } from './ArtifactDownloadPanel';
import { inDesktop, openPath } from './desktop';
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
  // A run in the user's own folder keeps no workspace; the project's folder is where the change is.
  const [projectFolder] = createResource(
    () => (patch() && typeof reason().workspace_kept_at !== 'string' ? props.itemId : undefined),
    async (id) => {
      try {
        const project = await api.projects.get((await api.items.get(id)).project_id);
        const own = project.workspace_mode === 'local_branch' || project.workspace_mode === 'in_place';
        return own && project.repository ? project.repository : null;
      } catch {
        return null;
      }
    },
  );
  const folder = () =>
    typeof reason().workspace_kept_at === 'string' ? (reason().workspace_kept_at as string) : (projectFolder() ?? null);
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
  const openFolder = async (path: string) => {
    if (!inDesktop()) return copy(path, 'Path');
    try {
      await openPath(path);
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
                {inDesktop() ? 'Open folder' : 'Copy path'}
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

const AttemptRow: Component<{ requestId: string; attempt: AttemptSummary; itemId?: string; numbered: boolean }> = (props) => {
  const [expanded, setExpanded] = createSignal(false);
  const stateInfo = () => describeExecutionState(props.attempt.state);
  const outcome = () => describeAttemptOutcome(props.attempt.state, props.attempt.terminal_reason);
  const provenance = () =>
    describeModelProvenance(
      props.attempt.model_provenance,
      (props.attempt.actual_execution as { model_observation_source?: string | null } | null)?.model_observation_source,
    );
  // The verdict as last read from the server: the prop's until a review is
  // submitted here, then the refetched attempt's.
  const [fresh, setFresh] = createSignal<AttemptSummary['review'] | undefined>(undefined);
  const review = () => (fresh() !== undefined ? fresh() : props.attempt.review);
  // The backend's `needs_review` condition: a terminal attempt that left a
  // patch or a kept workspace.
  const patchBytes = () => {
    const arts = (props.attempt.terminal_reason as { artifacts?: unknown } | null)?.artifacts;
    const patch = (Array.isArray(arts) ? arts : []).find((a) => a && (a as { kind?: unknown }).kind === 'patch') as
      | { size_bytes?: unknown }
      | undefined;
    return typeof patch?.size_bytes === 'number' ? patch.size_bytes : 0;
  };
  const reviewable = () =>
    isTerminalAttemptState(props.attempt.state) &&
    (patchBytes() > 0 || !!(props.attempt.terminal_reason as { workspace_kept_at?: unknown } | null)?.workspace_kept_at);
  const awaitingReview = () => reviewable() && !review();
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
      window.dispatchEvent(new CustomEvent(ITEM_UPDATED_EVENT));
    } catch (e) {
      setReviewError(e instanceof Error ? e.message : 'The verdict could not be saved');
    } finally {
      setReviewing(false);
    }
  };
  const economics = () =>
    formatUsageEconomics(props.attempt.usage_economics, isTerminalAttemptState(props.attempt.state));

  return (
    <li data-testid="attempt" class="space-y-4 rounded-[20px] p-4" style={{ 'background-color': 'var(--color-bg-app)', 'box-shadow': 'var(--shadow-sm)' }}>
      <div class="flex flex-wrap items-center gap-2">
        <Show when={props.numbered}>
          <span class="font-heading text-lg" style={{ color: 'var(--color-text-primary)' }}>
            Attempt {props.attempt.attempt_number}
          </span>
        </Show>
        <Show
          when={reviewable()}
          fallback={<Badge tone={outcome().badge?.tone ?? stateInfo().tone}>{outcome().badge?.label ?? stateInfo().label}</Badge>}
        >
          <Show
            when={review()}
            fallback={<Badge tone="warning">
                {patchBytes() > 0 ? 'Finished — needs your review' : 'Finished — changes could not be read, needs your review'}
              </Badge>}
          >
            {(r) => <Badge tone={r().verdict === 'accepted' ? 'success' : 'neutral'}>{r().verdict === 'accepted' ? 'Accepted' : 'Rejected'}</Badge>}
          </Show>
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
          leased {relativeTimeFromIso(props.attempt.lease_issued_at)}
        </span>
        <span class="ml-auto max-w-[14rem] truncate text-[11px]" title={props.attempt.runner_id} style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
          runner {props.attempt.runner_id}
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

      {/* Model provenance — a distinct, honest tone per case, never a bare
          "matched" boolean. */}
      <div class="flex items-start gap-2 text-xs">
        <Badge tone={provenance().tone}>{provenance().label}</Badge>
        <span class="pt-0.5" style={{ color: 'var(--color-text-secondary)' }}>{provenance().detail}</span>
      </div>

      {/* Usage/economics — every dollar figure honestly labeled, "Not
          measured" rendered as literal text, never $0.00. One tile per
          figure, never summed. */}
      <dl class="grid grid-cols-1 gap-2 sm:grid-cols-3">
        <CostTile label="Cost" value={economics().modelTokenCostUsd} />
        <CostTile label="Time" value={economics().runnerTime.wallClock} />
        <CostTile label="Time" value={economics().runnerTime.costUsd} />
      </dl>

      <Button size="sm" variant="ghost" onClick={() => setExpanded((v) => !v)} aria-expanded={expanded()}>
        {expanded() ? 'Hide details' : 'Show timeline, questions & files'}
      </Button>

      <Show when={expanded()}>
        <div class="space-y-5 border-t pt-4" style={{ 'border-color': 'var(--color-border-light)' }}>
          <section>
            <h4 class="mb-2 text-base" style={{ color: 'var(--color-text-primary)' }}>
              Timeline
            </h4>
            <EventTimeline requestId={props.requestId} attemptNumber={props.attempt.attempt_number} />
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

/** One cost/usage figure as a tile. An unmeasured figure keeps its literal
 *  "Not measured" text and is set apart (italic, dashed underline) so it can
 *  never be read as a real amount. */
const CostTile: Component<{ label: string; value: string }> = (props) => {
  const unmeasured = () => props.value === NOT_MEASURED_TEXT;
  return (
    <div class="rounded-[20px] px-3.5 py-3" style={{ 'background-color': 'var(--color-bg-panel)' }}>
      <dt class="text-[11px]" style={{ color: 'var(--color-text-tertiary)' }}>
        {props.label}
      </dt>
      <dd class="mt-0.5 text-sm">
        <span
          class={unmeasured() ? 'inline-block border-b border-dashed italic' : 'font-semibold'}
          style={{
            color: unmeasured() ? 'var(--color-text-secondary)' : 'var(--color-text-primary)',
            'border-color': 'var(--color-border-medium)',
          }}
        >
          {props.value}
        </span>
      </dd>
    </div>
  );
};

/**
 * Every attempt made against one execution request, reading real data from
 * `store.ts#attemptsFor`/`loadAttempts`. Each row exposes model provenance,
 * usage economics, and (on demand, to avoid an eager fetch for every attempt
 * of every visible request) its normalized event timeline, decision inbox,
 * and artifact download action.
 */
const AttemptList: Component<AttemptListProps> = (props) => (
  <ul class="space-y-2">
    <For each={props.attempts}>
      {(attempt) => <AttemptRow requestId={props.requestId} attempt={attempt} itemId={props.itemId} numbered={props.attempts.length > 1} />}
    </For>
  </ul>
);

export default AttemptList;
