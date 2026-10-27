import { type Component, For, Show, createMemo, createResource, createSignal } from 'solid-js';
import { Badge, Button, LoadError, Modal } from '../ui';
import { toast } from '../ui/toast';
import { api } from '../api';
import { artifactsApi, attemptsApi, type AttemptSummary } from '../execution';
import type { RunLimit } from '../execution/api';
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
  changedNoFile,
  needsVerdict,
  isTerminalAttemptState,
  limitShare,
  readAttemptUsage,
  formatTokenGroups,
} from './attemptFormat';
import ArtifactDownloadPanel, { ArtifactView } from './ArtifactDownloadPanel';
import { invoke } from '@tauri-apps/api/core';
import { request } from '../api/client';
import { SANDBOX_TEXT } from '../help/texts';
import { inDesktop } from './desktop';
import { parseRunLog } from './runLog';
import DiffView from './DiffView';
import DecisionInbox from './DecisionInbox';
import MrpPanel from './MrpPanel';
import MergeButton from './MergeButton';
import ReviewActions from './ReviewActions';
import PlanPanel from './PlanPanel';
import RunChips from './RunChips';
import { needsAPerson } from './RerunAtL3';
import { AgentConcluded, AgentDid, SideSlot } from './AgentWork';
import { describeExecutionState, describePullRequestState, relativeTimeFromIso } from './shared';

export interface AttemptListProps {
  requestId: string;
  attempts: AttemptSummary[];
  /** The item the request belongs to; the plan panel creates subtasks under it. */
  itemId?: string;
  /** Called once a review verdict is saved, so the host can refresh the run's status. */
  onReviewed?: () => void;
  /** The run's token limit, whose share the folded usage shows. */
  limit?: RunLimit;
  /** `page`: what the agent got, its usage and the ids go to `sideTarget`, the page's side
   *  column. `drawer` (the default): one column, in that same order. */
  layout?: 'drawer' | 'page';
  sideTarget?: HTMLElement;
}

async function copyText(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast.success(`${what} copied.`);
  } catch {
    toast.error(`Couldn't copy the ${what.toLowerCase()}: ${text}`);
  }
}

/** The pack panel, shown only when the attempt's artifacts include a
 *  merge-readiness pack (kind `mrp`). */
const MrpSlot: Component<{ requestId: string; attemptNumber: number; merge?: AttemptSummary['merge'] }> = (props) => {
  const [artifacts, { refetch }] = createResource(
    () => `${props.requestId}:${props.attemptNumber}`,
    () => artifactsApi.list(props.requestId, props.attemptNumber),
  );
  return (
    <Show
      when={!artifacts.error}
      fallback={<LoadError what="the merge-readiness pack" error={artifacts.error} onRetry={() => void refetch()} />}
    >
      <Show when={(artifacts() ?? []).some((a) => a.kind === 'mrp')}>
        <section>
          <h4 class="mb-2 text-base" style={{ color: 'var(--color-text-primary)' }}>
            Merge-readiness pack
          </h4>
          <MrpPanel requestId={props.requestId} attemptNumber={props.attemptNumber} merge={props.merge} />
        </section>
      </Show>
    </Show>
  );
};


/** What the agent was given: its sandbox, the names of its environment variables, where it
 *  worked, and (collapsed) the exact prompt it read. */
const AgentGot: Component<{ requestId: string; attempt: AttemptSummary; workspace: string | null }> = (props) => {
  const [opened, setOpened] = createSignal(false);
  const [instructions, { refetch: refetchInstructions }] = createResource(
    () => (opened() ? props.requestId : undefined),
    async (id) =>
      (await request<{ instructions?: string | null }>(`/executions/${encodeURIComponent(id)}`)).instructions ?? null,
  );
  const sandbox = () => SANDBOX_TEXT[props.attempt.sandbox as keyof typeof SANDBOX_TEXT] ?? null;
  const names = () => props.attempt.environment_names ?? [];
  return (
    <section data-testid="agent-got" class="space-y-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
      <h4 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>What the agent got</h4>
      <p data-testid="agent-got-sandbox">{sandbox() ?? 'The runner did not say whether the agent was sandboxed'}</p>
      <p data-testid="agent-got-env" class="break-all">
        Environment variables: {names().length > 0 ? names().join(', ') : 'none reported'}
      </p>
      <Show when={props.workspace}>
        {(w) => (
          <p data-testid="agent-got-workspace" class="break-all" title={w()}>
            Workspace: the project's folder
          </p>
        )}
      </Show>
      <details onToggle={(e) => setOpened(e.currentTarget.open)}>
        <summary class="cursor-pointer text-xs font-semibold" style={{ color: 'var(--color-accent-ink)' }}>What the agent read</summary>
        <Show when={instructions.loading}><p class="text-xs">Loading the prompt…</p></Show>
        <Show when={instructions.error}>
          <LoadError what="what the agent read" error={instructions.error} onRetry={() => void refetchInstructions()} />
        </Show>
        <Show when={!instructions.error && instructions()}>
          {(text) => (
            <pre data-testid="agent-read" class="mt-1 max-h-72 overflow-auto whitespace-pre-wrap rounded-item px-4 py-3 text-xs"
              style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)' }}>{text()}</pre>
          )}
        </Show>
      </details>
    </section>
  );
};

/** One id the support or a bug report asks for, with a button that copies it. */
const IdRow: Component<{ label: string; value: string }> = (props) => (
  <div class="flex items-center gap-2">
    <dt class="w-14 flex-none" style={{ color: 'var(--color-text-tertiary)' }}>{props.label}</dt>
    <dd class="m-0 flex min-w-0 flex-1 items-center gap-2">
      <span class="min-w-0 flex-1 truncate" title={props.value} style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-secondary)' }}>
        {props.value}
      </span>
      <button
        type="button"
        aria-label={`Copy the ${props.label.toLowerCase()} id`}
        class="min-h-6 min-w-6 flex-none px-1 text-xs font-semibold hover:underline"
        style={{ color: 'var(--color-accent-ink)' }}
        onClick={() => void copyText(props.value, `${props.label} id`)}
      >
        Copy
      </button>
    </dd>
  </div>
);

/** What the run left to look at: its diff, its folder and branch; what the agent concluded and
 *  did; and, beside the run, what the agent got, what it used and the ids with the raw log. */
const AttemptTools: Component<{
  requestId: string;
  attempt: AttemptSummary;
  itemId?: string;
  limit?: RunLimit;
  /** This is the run's newest attempt, the one the limit's live count belongs to. */
  latest: boolean;
  page: boolean;
  sideTarget?: HTMLElement;
}> = (props) => {
  const n = () => props.attempt.attempt_number;
  const terminal = () => isTerminalAttemptState(props.attempt.state);
  const reason = () => (props.attempt.terminal_reason ?? {}) as { workspace_kept_at?: unknown; result?: unknown };
  const [artifacts, { refetch: refetchArtifacts }] = createResource(
    () => (terminal() ? `${props.requestId}:${n()}` : undefined),
    () => artifactsApi.list(props.requestId, n()),
  );
  // A failed list is said as one (below); the buttons it feeds are simply not offered meanwhile.
  const listed = () => (artifacts.error ? [] : (artifacts() ?? []));
  const patch = () => listed().find((a) => a.kind === 'patch');
  const log = () => listed().find((a) => a.kind === 'log' || (a.name ?? '').endsWith('.log'));
  const plan = () => listed().find((a) => a.kind === 'plan');
  const evidence = () => listed().find((a) => a.kind === 'evidence');
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
  // The runner's workspace: the folder the agent worked in. Every path this run shows is
  // given inside it.
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
  const canOpen = () => inDesktop() && repository() !== null;
  const openFolder = async (path: string) => {
    if (!canOpen()) return copyText(path, 'Path');
    try {
      await invoke('open_folder', { path, repository: repository() });
    } catch (err) {
      toast.error(`Couldn't open ${path}: ${err instanceof Error ? err.message : String(err)}`);
    }
  };
  const patchHasChanges = () => (patch()?.size_bytes ?? 0) > 0;

  // What the run used, in one line for the fold: the tokens counted against a limit and, when
  // the run has one, the share of it.
  const usageLine = () => {
    const u = readAttemptUsage(props.attempt);
    let tokens: string;
    if (u.counted !== null) tokens = `${formatTokenGroups(u.counted)} tokens counted`;
    else if (u.tokensIn !== null || u.tokensOut !== null) {
      tokens = `${u.tokensIn === null ? '?' : formatTokenCount(u.tokensIn)} in · ${u.tokensOut === null ? '?' : formatTokenCount(u.tokensOut)} out`;
    } else tokens = `Tokens: ${NOT_MEASURED_TEXT}`;
    const used = u.counted ?? (props.latest ? props.limit?.used ?? null : null);
    const share = limitShare(used, props.limit?.tokens);
    return share === null ? tokens : `${tokens} · ${share}% of its limit`;
  };

  return (
    <>
      <Show when={terminal() && artifacts.error}>
        <LoadError what="the files from this run" error={artifacts.error} onRetry={() => void refetchArtifacts()} />
      </Show>
      <Show when={terminal()}>
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
              <Button size="sm" variant="secondary" onClick={() => void copyText(b(), 'Branch')}>Copy branch</Button>
            )}
          </Show>
        </div>
        <AgentConcluded result={result()} workspace={folder()} />
      </Show>
      <AgentDid
        requestId={props.requestId}
        attemptNumber={n()}
        log={steps()}
        reading={steps.loading}
        terminal={terminal()}
        workspace={folder()}
      />
      <Show when={terminal()}>
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
      </Show>
      <SideSlot page={props.page} target={props.sideTarget}>
        <div data-testid="attempt-side" class="space-y-4">
          <Show when={terminal()}>
            <AgentGot requestId={props.requestId} attempt={props.attempt} workspace={folder()} />
            <details data-testid="attempt-usage-fold" class="rounded-item px-4 py-3" style={{ 'background-color': 'var(--color-bg-panel)' }}>
              <summary class="cursor-pointer text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                Usage <span data-testid="usage-summary" class="font-normal" style={{ color: 'var(--color-text-secondary)' }}>· {usageLine()}</span>
              </summary>
              <div class="mt-2">
                <UsagePanel attempt={props.attempt} />
              </div>
            </details>
          </Show>
          <section data-testid="attempt-ids" class="space-y-1.5 text-xs">
            <h4 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>Ids</h4>
            <dl class="space-y-1">
              <IdRow label="Request" value={props.requestId} />
              <IdRow label="Attempt" value={props.attempt.attempt_id} />
            </dl>
            <Button
              size="sm"
              variant="secondary"
              disabled={!log()}
              title={log() ? undefined : terminal() ? 'This run left no log' : 'The log is written when the run ends'}
              onClick={() => setLogOpen(true)}
            >
              Raw log
            </Button>
          </section>
        </div>
      </SideSlot>
    </>
  );
};

const AttemptRow: Component<{
  requestId: string;
  attempt: AttemptSummary;
  itemId?: string;
  numbered: boolean;
  limit?: RunLimit;
  latest: boolean;
  page: boolean;
  sideTarget?: HTMLElement;
  onReviewed?: () => void;
}> = (props) => {
  const [expanded, setExpanded] = createSignal(false);
  // The agent stopped to ask a person, or ended because it needed one: the question is one press away.
  const personNeeded = () => props.attempt.state === 'waiting_decision' || needsAPerson(props.attempt);
  let questions: HTMLHeadingElement | undefined;
  const openQuestion = () => {
    setExpanded(true);
    questions?.focus();
    questions?.scrollIntoView?.({ block: 'center' });
  };
  const stateInfo = () => describeExecutionState(props.attempt.state);
  const outcome = () => describeAttemptOutcome(props.attempt.state, props.attempt.terminal_reason);
  const harnessKind = () => (props.attempt.actual_execution as { harness_kind?: string } | null)?.harness_kind ?? null;
  // The verdict as last read from the server: the prop's until a review is
  // submitted here, then the refetched attempt's.
  const [fresh, setFresh] = createSignal<AttemptSummary['review'] | undefined>(undefined);
  const review = () => (fresh() !== undefined ? fresh() : props.attempt.review);
  const status = () => describeAttemptStatus(props.attempt, review());
  const showMerge = () => terminal() && !!merge() && !merge()!.merged && !changedNoFile(props.attempt);
  const terminal = () => isTerminalAttemptState(props.attempt.state);
  // The merge state as last read: the prop's until a merge is pressed here, then the refetched attempt's.
  const [freshMerge, setFreshMerge] = createSignal<AttemptSummary['merge'] | undefined>(undefined);
  const merge = () => freshMerge() ?? props.attempt.merge;
  const refreshMerge = async () => {
    try {
      const list = await attemptsApi.list(props.requestId);
      setFreshMerge(list.data.data.find((a) => a.attempt_number === props.attempt.attempt_number)?.merge);
    } catch {
      /* the next load shows it */
    }
    props.onReviewed?.();
    window.dispatchEvent(new CustomEvent(ITEM_UPDATED_EVENT));
  };
  return (
    <li data-testid="attempt" class="space-y-4 rounded-item p-4" style={{ 'background-color': 'var(--color-bg-app)', 'box-shadow': 'var(--shadow-sm)' }}>
      {/* The review banner sits above the run: the verdict, with its optional note, is asked
          for here and nowhere else on this tab. */}
      <Show when={needsVerdict(props.attempt)}>
        <section
          data-testid="review-banner"
          class="space-y-2 rounded-item px-4 py-3"
          style={{ 'background-color': review() ? 'var(--color-bg-panel)' : 'var(--color-warning-100)' }}
        >
          <Show when={!review()}>
            <p class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>This run is waiting for your verdict</p>
            <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
              Accept it, or reject it with a note saying why. The note is optional.
            </p>
          </Show>
          <ReviewActions
            requestId={props.requestId}
            attemptNumber={props.attempt.attempt_number}
            review={review()}
            onReviewed={(r) => {
              setFresh(r);
              props.onReviewed?.();
            }}
          />
        </section>
      </Show>

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
        <span class="ml-auto max-w-[14rem] truncate text-micro" title={`Runner ${props.attempt.runner_id}`} style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
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
      <Show when={personNeeded()}>
        <Button size="sm" variant="secondary" onClick={openQuestion}>
          Open the question
        </Button>
      </Show>
      <Show when={outcome().badge?.detail}>
        {(detail) => (
          <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            {detail()}
          </p>
        )}
      </Show>

      <Show when={showMerge()}>
        <MergeButton attemptId={props.attempt.attempt_id} merge={merge()!} onMerged={() => void refreshMerge()} />
      </Show>

      <Show when={!terminal()}>
        <p data-testid="attempt-working" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          {props.attempt.state === 'waiting_decision'
            ? 'The agent is waiting for your answer — see Questions from the agent below.'
            : 'The agent is working. Its steps appear in the timeline as it takes them; the tokens it used appear here when it finishes.'}
        </p>
      </Show>

      <AttemptTools
        requestId={props.requestId}
        attempt={props.attempt}
        itemId={props.itemId}
        limit={props.limit}
        latest={props.latest}
        page={props.page}
        sideTarget={props.sideTarget}
      />

      <Button size="sm" variant="ghost" onClick={() => setExpanded((v) => !v)} aria-expanded={expanded()}>
        {expanded() ? 'Hide details' : 'Show questions & files'}
      </Button>

      <Show when={expanded()}>
        <div class="space-y-5 border-t pt-4" style={{ 'border-color': 'var(--color-border-light)' }}>
          <section>
            <h4 ref={questions} tabIndex={-1} class="mb-2 text-base" style={{ color: 'var(--color-text-primary)' }}>
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

          <MrpSlot requestId={props.requestId} attemptNumber={props.attempt.attempt_number} merge={merge()} />
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
    if (u.cacheRead !== null) parts.push(`${formatTokenCount(u.cacheRead)} read from cache (not counted)`);
    if (u.cacheWrite !== null) parts.push(`${formatTokenCount(u.cacheWrite)} written to cache`);
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
    <section data-testid="attempt-usage" aria-label="Usage" class="space-y-2 rounded-item px-4 py-3" style={{ 'background-color': 'var(--color-bg-panel)' }}>
      <div class="flex flex-wrap items-baseline gap-x-4 gap-y-1">
        <Show
          when={usage().tokensIn !== null || usage().tokensOut !== null}
          fallback={<span class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>Tokens: <Unmeasured /></span>}
        >
          <span data-testid="usage-tokens" class="text-base font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            <Show
              when={usage().counted !== null}
              fallback={
                <>
                  <span title={`${exact(usage().tokensIn)} tokens in`}>{usage().tokensIn === null ? '?' : formatTokenCount(usage().tokensIn!)} tokens in</span>
                  {' · '}
                  <span title={`${exact(usage().tokensOut)} tokens out`}>{usage().tokensOut === null ? '?' : formatTokenCount(usage().tokensOut!)} out</span>
                </>
              }
            >
              <span data-testid="usage-counted" title={`${exact(usage().counted)} tokens counted against a limit: the input less the cache reads, plus the output`}>
                {formatTokenGroups(usage().counted!)} tokens counted
              </span>
              <span
                data-testid="usage-in-out"
                class="ml-2 text-xs font-normal"
                title={`${exact(usage().tokensIn)} tokens in (cache reads included), ${exact(usage().tokensOut)} out`}
                style={{ color: 'var(--color-text-secondary)' }}
              >
                {usage().tokensIn === null ? '?' : formatTokenGroups(usage().tokensIn!)} in · {usage().tokensOut === null ? '?' : formatTokenGroups(usage().tokensOut!)} out
              </span>
            </Show>
          </span>
        </Show>
        <Show
          when={cost() !== NOT_MEASURED_TEXT}
          fallback={<span data-testid="usage-cost" class="text-sm" title={`${harness()} did not report a cost`} style={{ color: 'var(--color-text-secondary)' }}>Cost: <Unmeasured /></span>}
        >
          <span data-testid="usage-cost" class="text-sm font-semibold" title={`${harness()}'s own estimate at list price — your bill may differ`} style={{ color: 'var(--color-text-primary)' }}>
            {cost()}
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
        <p class="text-micro" style={{ color: 'var(--color-text-tertiary)' }}>
          The cost is {harness()}'s own estimate at list price, not your bill.
        </p>
      </Show>
    </section>
  );
};


/**
 * The attempts made against one execution request, reading real data from
 * `store.ts#attemptsFor`/`loadAttempts`. A run with several attempts offers chips to pick
 * one, the newest by default, and shows only the one picked. The row exposes model
 * provenance, usage and the run's timeline, and (on demand, to avoid an eager fetch for
 * every attempt) its decision inbox and artifact download action.
 */
const AttemptList: Component<AttemptListProps> = (props) => {
  const newest = createMemo(() =>
    props.attempts.reduce<AttemptSummary | undefined>((max, x) => (!max || x.attempt_number > max.attempt_number ? x : max), undefined),
  );
  // The picked attempt follows its id, not its object: a refresh that brings the same attempt
  // changed (a new event moves its checkpoint) updates its row in place, so what the person
  // opened on it stays open. Until one is picked, the newest is shown, so a new attempt
  // takes its place.
  const [picked, setPicked] = createSignal<string | undefined>(undefined);
  const activeId = createMemo(() => {
    const p = picked();
    return p && props.attempts.some((a) => a.attempt_id === p) ? p : newest()?.attempt_id;
  });
  const byId = createMemo(() => new Map(props.attempts.map((attempt) => [attempt.attempt_id, attempt])));
  const chips = createMemo(() =>
    [...props.attempts]
      .sort((a, b) => b.attempt_number - a.attempt_number)
      .map((a) => {
        const status = describeAttemptStatus(a, a.review);
        return { id: a.attempt_id, label: `Attempt ${a.attempt_number}`, tone: status.tone, state: status.label };
      }),
  );
  return (
    <div class="space-y-3">
      <Show when={props.attempts.length > 1}>
        <RunChips chips={chips()} active={activeId() ?? ''} onPick={setPicked} label="Attempts" testId="attempt-chips" />
      </Show>
      <ul class="space-y-2">
        <Show when={activeId()} keyed>
          {(id) => (
            <Show when={byId().get(id)}>
              {(attempt) => (
                <AttemptRow
                  requestId={props.requestId}
                  attempt={attempt()}
                  itemId={props.itemId}
                  numbered={props.attempts.length > 1}
                  limit={props.limit}
                  latest={attempt().attempt_id === newest()?.attempt_id}
                  page={props.layout === 'page'}
                  sideTarget={props.sideTarget}
                  onReviewed={props.onReviewed}
                />
              )}
            </Show>
          )}
        </Show>
      </ul>
    </div>
  );
};

export default AttemptList;
