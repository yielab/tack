import { type Component, For, type JSX, Show, createMemo, createResource, createSignal } from 'solid-js';
import { Badge, Button, Fold, HelpHint, LoadError, Modal, type DocketRunWord, type RunStateName } from '../ui';
import { toast } from '../ui/toast';
import { truncateMiddle } from '../ui/truncateMiddle';
import { api } from '../api';
import { artifactsApi, attemptsApi, isFolderRun, readRunRecords, readRunSteps, type AttemptSummary } from '../execution';
import type { RunLimit } from '../execution/api';
import { ITEM_UPDATED_EVENT } from '../state/itemEvents';
import {
  NOT_MEASURED_TEXT,
  describeAttemptOutcome,
  describeAttemptStatus,
  neverStarted,
  describeModelProvenance,
  formatTokenCount,
  formatWallClock,
  harnessName,
  changedNoFile,
  needsVerdict,
  isTerminalAttemptState,
  limitShare,
  patchStats,
  readAttemptUsage,
  formatTokenGroups,
} from './attemptFormat';
import ArtifactDownloadPanel, { ArtifactView } from './ArtifactDownloadPanel';
import { invoke } from '@tauri-apps/api/core';
import { request } from '../api/client';
import { HELP, SANDBOX_TEXT } from '../help/texts';
import { inDesktop } from './desktop';
import { parseRunLog } from './runLog';
import DiffView from './DiffView';
import DecisionInbox from './DecisionInbox';
import MrpPanel, { MergeState, MrpVerdict, SignatureState, buildChecks, checksSummary, createMrp } from './MrpPanel';
import MergeButton from './MergeButton';
import ReviewActions from './ReviewActions';
import PlanPanel from './PlanPanel';
import RunChips from './RunChips';
import RunSection from './RunSection';
import UsageMeter from './UsageMeter';
import { needsAPerson } from './RerunAtL3';
import { AgentConcluded, AgentDid, SideSlot, StepsRan, TeamRecords } from './AgentWork';
import { AttemptTeamSteps } from './TeamSteps';
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
  /** The profile and agent the run was made with: the step track reads its team from them. */
  profileId?: string | null;
  agentKind?: string | null;
  /** The run's "Run again", drawn by the caller (it knows the task); given how filled to draw it,
   *  since only one button of the verdict is filled. */
  runAgain?: (variant: 'primary' | 'secondary') => JSX.Element;
  /** `page`: what the agent got, its usage and the ids go to `sideTarget`, the page's side
   *  column. `drawer` (the default): one column, in that same order. */
  layout?: 'drawer' | 'page';
  sideTarget?: HTMLElement;
  /** The item's Now panel is on screen and shows this very run: what it already says (the state in
   *  words, the token meter) is not said again here. */
  nowShown?: boolean;
}

async function copyText(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast.success(`${what} copied.`);
  } catch {
    toast.error(`Couldn't copy the ${what.toLowerCase()}: ${text}`);
  }
}

/**
 * An attempt in the one language every screen uses for where a run is. The docket words stay as the
 * banner's second line: a run that waits for a person, one that ended because it needed one, one that
 * wants a verdict, one that changed nothing.
 */
export function attemptRunState(
  attempt: AttemptSummary,
  review: AttemptSummary['review'] | undefined = attempt.review,
): { state: RunStateName; docket?: DocketRunWord } {
  if (attempt.state === 'waiting_decision') return { state: 'needs', docket: 'Needs a person' };
  if (!isTerminalAttemptState(attempt.state)) return { state: 'running' };
  if (attempt.state === 'cancelled') return { state: 'cancelled' };
  if (attempt.state !== 'succeeded') {
    if (needsAPerson(attempt)) return { state: 'needs', docket: 'Needs a person' };
    return neverStarted(attempt) ? { state: 'notstarted' } : { state: 'failed' };
  }
  if (attempt.merge?.merged || attempt.pull_request?.state === 'merged') return { state: 'done' };
  if (needsVerdict(attempt) && !review) return { state: 'review', docket: 'Needs your review' };
  // A folder team has no repository to change, so having changed nothing there is not news.
  return changedNoFile(attempt) && !isFolderRun(attempt.terminal_reason) ? { state: 'done', docket: 'no changes recorded' } : { state: 'done' };
}

/** The exact prompt the agent read, fetched when its fold first opens, with the fingerprint of
 *  the wording around the card's text. */
const AgentRead: Component<{ requestId: string }> = (props) => {
  const [read, { refetch: refetchRead }] = createResource(
    () => props.requestId,
    async (id) => {
      const run = await request<{ instructions?: string | null; prompt_template_sha?: string | null }>(`/executions/${encodeURIComponent(id)}`);
      return { instructions: run.instructions ?? null, sha: run.prompt_template_sha ?? null };
    },
  );
  return (
    <>
      <Show when={read.loading}><p class="text-xs">Loading the prompt…</p></Show>
      <Show when={read.error}>
        <LoadError what="what the agent read" error={read.error} onRetry={() => void refetchRead()} />
      </Show>
      <Show when={!read.error && read()?.sha}>
        {(sha) => (
          <p data-testid="agent-read-fingerprint" class="mt-1 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            Prompt fingerprint{' '}
            <span title={sha()} style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-secondary)' }}>{sha().slice(0, 8)}</span>
            <HelpHint label="Prompt fingerprint" help={HELP.promptFingerprint} />
          </p>
        )}
      </Show>
      <Show when={!read.error && read()?.instructions}>
        {(text) => (
          <pre data-testid="agent-read" class="mt-1 max-h-72 overflow-auto whitespace-pre-wrap rounded-item px-4 py-3 text-xs"
            style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)' }}>{text()}</pre>
        )}
      </Show>
    </>
  );
};

/** What the agent was given: its sandbox, the names of its environment variables, where it
 *  worked, and (collapsed) the exact prompt it read. */
const AgentGot: Component<{ requestId: string; attempt: AttemptSummary; workspace: string | null }> = (props) => {
  const sandbox = () => SANDBOX_TEXT[props.attempt.sandbox as keyof typeof SANDBOX_TEXT] ?? null;
  const names = () => props.attempt.environment_names ?? [];
  return (
    <section data-testid="agent-got" class="space-y-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
      <h4 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>What the agent got</h4>
      <p data-testid="agent-got-sandbox">{sandbox() ?? 'The runner did not say whether the agent was sandboxed'}</p>
      <Show when={names().length > 0}>
        <p data-testid="agent-got-env" class="break-all">
          Environment variables: {names().join(', ')}
        </p>
      </Show>
      <Show when={props.workspace}>
        {(w) => (
          <p data-testid="agent-got-workspace" class="break-all" title={w()}>
            Workspace: the project's folder
          </p>
        )}
      </Show>
      <Fold kind="agent-read" size="sub" title="What the agent read" lazy>
        <AgentRead requestId={props.requestId} />
      </Fold>
    </section>
  );
};

/** One id the support or a bug report asks for, with a button that copies it. */
const IdRow: Component<{ label: string; value: string }> = (props) => (
  <div class="flex items-center gap-2">
    <dt class="w-14 flex-none" style={{ color: 'var(--color-text-tertiary)' }}>{props.label}</dt>
    <dd class="m-0 flex min-w-0 flex-1 items-center gap-2">
      <span class="min-w-0 flex-1 truncate" title={props.value} style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-secondary)' }}>
        {truncateMiddle(props.value)}
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

/** How long the attempt ran: the runner's own clock, else the span between its two timestamps. */
function durationOf(attempt: AttemptSummary, wallClockMs: number | null): string | null {
  if (wallClockMs !== null) return formatWallClock(wallClockMs, true);
  const from = attempt.started_at ? Date.parse(attempt.started_at) : NaN;
  const to = attempt.ended_at ? Date.parse(attempt.ended_at) : NaN;
  return Number.isNaN(from) || Number.isNaN(to) ? null : formatWallClock(Math.max(0, to - from), true);
}

/**
 * One attempt, top-down: the verdict (what it is, what the verifier thinks, what it cost, whether it
 * merged, and the one thing to do now), then what needs the person, the checks it passed, the
 * changes it made, the steps it took, the files it left, and last, behind a fold, the ids and the
 * raw log. Every section that has nothing to say is still there to say so.
 */
const AttemptRow: Component<{
  requestId: string;
  attempt: AttemptSummary;
  itemId?: string;
  numbered: boolean;
  limit?: RunLimit;
  latest: boolean;
  page: boolean;
  sideTarget?: HTMLElement;
  profileId?: string | null;
  agentKind?: string | null;
  runAgain?: (variant: 'primary' | 'secondary') => JSX.Element;
  onReviewed?: () => void;
  nowShown?: boolean;
}> = (props) => {
  // Now speaks for the run's newest attempt only.
  const dupe = () => !!props.nowShown && props.latest;
  const n = () => props.attempt.attempt_number;
  const terminal = () => isTerminalAttemptState(props.attempt.state);
  const asking = () => props.attempt.state === 'waiting_decision';
  // The agent stopped to ask a person, or ended because it needed one: the question is one press away.
  const personNeeded = () => asking() || needsAPerson(props.attempt);
  const reason = () => (props.attempt.terminal_reason ?? {}) as { workspace_kept_at?: unknown; result?: unknown };
  const stateInfo = () => describeExecutionState(props.attempt.state);
  const outcome = () => describeAttemptOutcome(props.attempt.state, props.attempt.terminal_reason);
  const harnessKind = () => (props.attempt.actual_execution as { harness_kind?: string } | null)?.harness_kind ?? null;
  const folderTeam = () => isFolderRun(props.attempt.terminal_reason);

  // The verdict as last read from the server: the prop's until a review is
  // submitted here, then the refetched attempt's.
  const [fresh, setFresh] = createSignal<AttemptSummary['review'] | undefined>(undefined);
  const review = () => (fresh() !== undefined ? fresh() : props.attempt.review);
  const status = () => describeAttemptStatus(props.attempt, review(), mrp.pack()?.recommendation.decision);
  // The merge state as last read: the prop's until a merge is pressed here, then the refetched attempt's.
  const [freshMerge, setFreshMerge] = createSignal<AttemptSummary['merge'] | undefined>(undefined);
  const merge = () => freshMerge() ?? props.attempt.merge;
  const showMerge = () => terminal() && !!merge() && !merge()!.merged && !changedNoFile(props.attempt) && !folderTeam();
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

  // The files the run left: listed once for the sections that read them.
  const [artifacts, { refetch: refetchArtifacts }] = createResource(
    () => `${props.requestId}:${n()}:${terminal()}`,
    () => artifactsApi.list(props.requestId, n()),
  );
  // A failed list is said as one (below); what it feeds is simply not offered meanwhile.
  const listed = () => (artifacts.error ? [] : (artifacts() ?? []));
  const patch = () => listed().find((a) => a.kind === 'patch');
  const log = () => listed().find((a) => a.kind === 'log' || (a.name ?? '').endsWith('.log'));
  const plan = () => listed().find((a) => a.kind === 'plan');
  const report = () => listed().find((a) => a.kind === 'report');
  const evidence = () => listed().find((a) => a.kind === 'evidence');
  const patchHasChanges = () => (patch()?.size_bytes ?? 0) > 0;
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
  // The patch is read as soon as there is one: the Changes section counts its files and lines, and the diff opens on it.
  const [patchText] = createResource(
    () => (patchHasChanges() ? patch() : undefined),
    async (p) => {
      try {
        return await (await artifactsApi.download(props.requestId, n(), p.artifact_id)).text();
      } catch {
        return null;
      }
    },
  );
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
  const runSteps = () => readRunSteps(props.attempt.terminal_reason);
  const records = () => (readRunRecords(props.attempt.terminal_reason) ?? []).filter((set) => set.rows.length > 0);
  const [steps] = createResource(log, async (l) => {
    try {
      return parseRunLog(await (await artifactsApi.download(props.requestId, n(), l.artifact_id)).text());
    } catch {
      return undefined;
    }
  });
  const [diffOpen, setDiffOpen] = createSignal(false);
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

  // What the verifier made of it, and what the item asked for: the pack is read when the run left one.
  const mrp = createMrp(() => (listed().some((a) => a.kind === 'mrp') ? { requestId: props.requestId, attemptNumber: n() } : undefined));
  const [brief] = createResource(
    () => (terminal() ? props.itemId : undefined),
    async (id) => {
      try {
        return await api.briefs.get(id);
      } catch {
        return null;
      }
    },
  );
  const doneWhen = () => (brief.error ? null : (brief()?.acceptance ?? null));
  const checks = () => buildChecks(mrp.pack(), doneWhen());

  // What the run used, in one line for the verdict and the fold: the tokens counted against a
  // limit and, when the run has one, the share of it.
  const usage = () => readAttemptUsage(props.attempt);
  const counted = () => usage().counted ?? (props.latest ? props.limit?.used ?? null : null);
  const tokensText = () => {
    const u = usage();
    if (u.counted !== null) return `${formatTokenGroups(u.counted)} tokens counted`;
    if (u.tokensIn !== null || u.tokensOut !== null) {
      return `${u.tokensIn === null ? '?' : formatTokenCount(u.tokensIn)} in · ${u.tokensOut === null ? '?' : formatTokenCount(u.tokensOut)} out`;
    }
    return `Tokens: ${NOT_MEASURED_TEXT}`;
  };
  const usageLine = () => {
    const share = limitShare(counted(), props.limit?.tokens);
    return share === null ? tokensText() : `${tokensText()} · ${share}% of its limit`;
  };
  const duration = () => durationOf(props.attempt, usage().wallClockMs);

  // An attempt that waits for an answer opens its activity by itself (the fold does), also when it starts to wait
  // while the page is open; "Open the question" opens it too, so the question has somewhere to be.
  const [activityKey, setActivityKey] = createSignal(0);
  let needsHeading: HTMLHeadingElement | undefined;
  let questionsHeading: HTMLHeadingElement | undefined;
  const openQuestion = () => {
    setActivityKey((k) => k + 1);
    const target = needsHeading ?? questionsHeading;
    target?.focus();
    target?.scrollIntoView?.({ block: 'center' });
  };

  // One filled button: answering what waits comes first, then the verdict, then the merge, then another run.
  const primary = (): 'answer' | 'review' | 'merge' | 'again' | null => {
    if (asking()) return 'answer';
    if (needsVerdict(props.attempt) && !review()) return 'review';
    if (showMerge() && merge()!.enabled) return 'merge';
    return terminal() ? 'again' : null;
  };

  const changesSummary = () => {
    const text = patchText();
    if (patchHasChanges()) {
      if (text) {
        const { files, added, removed } = patchStats(text);
        return `${files} ${files === 1 ? 'file' : 'files'} changed · +${added} −${removed}`;
      }
      return patchText.loading ? 'Reading the changes…' : 'The changes could not be read.';
    }
    // A folder team's verdict says it worked in a folder; the section does not say so again.
    return folderTeam() ? undefined : 'This run changed no files.';
  };

  return (
    <li data-testid="attempt" class="min-w-0 space-y-4">
      <section data-testid="run-verdict" aria-label="Verdict" class="min-w-0 space-y-3">
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
        </div>

        <Show when={outcome().whyStopped}>
          {(why) => (
            <p class="text-sm" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>
              <strong>Why it stopped:</strong> {why()}
            </p>
          )}
        </Show>
        <Show when={outcome().badge?.detail}>
          {(detail) => (
            <p class="text-xs" title={outcome().badge?.raw} style={{ color: 'var(--color-text-secondary)' }}>
              {detail()}
            </p>
          )}
        </Show>

        <Show when={mrp.pack()}>{(pack) => <MrpVerdict pack={pack()} />}</Show>

        <Show when={terminal() && !neverStarted(props.attempt)}>
          <div data-testid="verdict-facts" class="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1.5 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            <Show
              when={props.limit?.tokens != null && counted() !== null && !dupe()}
              fallback={<span data-testid="verdict-tokens">{tokensText()}</span>}
            >
              <div class="min-w-0" style={{ flex: '1 1 8rem' }}>
                <UsageMeter testId="verdict-meter" used={counted()!} limit={props.limit!.tokens!} />
              </div>
            </Show>
            <Show when={duration()}>
              {(text) => <span data-testid="verdict-duration">{text()}</span>}
            </Show>
          </div>

          <div data-testid="verdict-merge" class="min-w-0 space-y-1 text-sm">
            <Show when={folderTeam()}>
              <p data-testid="run-folder" class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                Works in a folder: nothing to merge
              </p>
            </Show>
            <Show when={!folderTeam()}>
              <Show when={merge()?.merged || (!showMerge() && props.attempt.state === 'succeeded' && merge())}>
                <MergeState merge={merge()!} />
              </Show>
              <Show when={showMerge()}>
                <div class="flex flex-wrap items-center gap-2">
                  <span class="text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>Not merged</span>
                  <MergeButton
                    attemptId={props.attempt.attempt_id}
                    merge={merge()!}
                    variant={primary() === 'merge' ? 'primary' : 'secondary'}
                    onMerged={() => void refreshMerge()}
                  />
                </div>
              </Show>
            </Show>
          </div>
        </Show>

        <Show when={!terminal() && !(dupe() && asking())}>
          <p data-testid="attempt-working" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            {asking()
              ? 'The agent is waiting for your answer — see Needs you below.'
              : 'The agent is working. Its steps appear in the timeline as it takes them; the tokens it used appear here when it finishes.'}
          </p>
        </Show>

        {/* The review banner: the verdict is asked for here and nowhere else on this tab. It is the run's one
            review, so it covers the pack on screen too; a rejection needs its reason. */}
        <Show when={needsVerdict(props.attempt)}>
          <section
            data-testid="review-banner"
            class="space-y-2 rounded-item px-4 py-3"
            style={{ 'background-color': review() ? 'var(--color-bg-panel)' : 'var(--color-warning-100)' }}
          >
            <Show when={!review()}>
              <p class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>This run is waiting for your verdict</p>
              <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                Accept it, or reject it. When you reject, say why.
              </p>
            </Show>
            <ReviewActions
              requestId={props.requestId}
              attemptNumber={props.attempt.attempt_number}
              review={review()}
              onReviewed={(r) => {
                setFresh(r);
                mrp.refresh();
                props.onReviewed?.();
              }}
            />
          </section>
        </Show>

        <div data-testid="verdict-actions" class="flex flex-wrap items-center gap-2">
          {/* The Now panel above already holds this button for the newest run; two for one question compete. */}
          <Show when={personNeeded() && !dupe()}>
            <Button size="sm" variant={primary() === 'answer' ? 'primary' : 'secondary'} onClick={openQuestion}>
              Open the question
            </Button>
          </Show>
          {props.runAgain?.(primary() === 'again' ? 'primary' : 'secondary')}
        </div>

        <Show when={terminal()}>
          <AgentConcluded result={result()} workspace={folder()} report={report()} requestId={props.requestId} attemptNumber={n()} />
        </Show>
      </section>

      <Show when={artifacts.error}>
        <LoadError what="the files from this run" error={artifacts.error} onRetry={() => void refetchArtifacts()} />
      </Show>

      <Show when={asking()}>
        <RunSection id="needs" title="Needs you" summary={dupe() ? undefined : 'The agent is waiting for your answer.'} headingRef={(el) => (needsHeading = el)}>
          <DecisionInbox
            requestId={props.requestId}
            attemptNumber={props.attempt.attempt_number}
            attemptId={props.attempt.attempt_id}
            attemptState={props.attempt.state}
            pendingOnly
            onResolved={() => props.onReviewed?.()}
          />
        </RunSection>
      </Show>

      <Show when={terminal() || mrp.pack()}>
        <RunSection id="checks" title="Checks" summary={checksSummary(checks())}>
          <MrpPanel mrp={mrp} doneWhen={doneWhen()} verdictAbove={needsVerdict(props.attempt) && !review()} />
        </RunSection>
      </Show>

      <Show when={terminal()}>
        <RunSection id="changes" title="Changes" summary={changesSummary()}>
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
          <Show when={plan() && props.itemId}>
            {(itemId) => (
              <PlanPanel
                requestId={props.requestId}
                attemptNumber={n()}
                artifactId={plan()!.artifact_id}
                itemId={itemId()}
                team={(props.attempt.actual_execution as { team?: string } | null)?.team}
              />
            )}
          </Show>
          <Modal isOpen={diffOpen()} onClose={() => setDiffOpen(false)} title="Changes" size="xl">
            <Show when={patchText()} fallback={<p class="text-sm">{patchText.loading ? 'Loading the diff…' : 'The diff could not be read.'}</p>}>
              {(t) => <DiffView patch={t()} />}
            </Show>
          </Modal>
        </RunSection>
      </Show>

      <RunSection
        id="steps"
        title="Steps"
        fold
        defaultOpen={!terminal() || props.attempt.state !== 'succeeded' || personNeeded()}
        summary={
          (runSteps() ?? []).length > 0
            ? `${runSteps()!.length} ${runSteps()!.length === 1 ? 'step' : 'steps'} ran.`
            : terminal()
              ? 'No steps were reported for this run.'
              : 'Steps appear as the team takes them.'
        }
      >
        <Show when={props.profileId || props.agentKind || (runSteps() ?? []).length > 0}>
          <AttemptTeamSteps
            requestId={props.requestId}
            attemptNumber={n()}
            live={!terminal()}
            profileId={props.profileId}
            agentKind={props.agentKind}
            hops={runSteps()}
            track={(runSteps() ?? []).length > 0}
          />
        </Show>
        <Show when={(runSteps() ?? []).length > 0}>
          <StepsRan steps={runSteps()!} />
        </Show>
        <Show when={records().length > 0}>
          <TeamRecords sets={records()} />
        </Show>
        <Fold
          kind="run-activity"
          testId="run-activity"
          size="sub"
          title="Activity"
          defaultOpen={!terminal() || asking()}
          needs={asking() ? 'a question needs you' : null}
          openRequest={activityKey()}
        >
          <div class="min-w-0 space-y-4">
            <AgentDid
              requestId={props.requestId}
              attemptNumber={n()}
              log={steps()}
              reading={steps.loading}
              terminal={terminal()}
              workspace={folder()}
            />
            <section>
              <DecisionInbox
                requestId={props.requestId}
                attemptNumber={props.attempt.attempt_number}
                attemptId={props.attempt.attempt_id}
                attemptState={props.attempt.state}
                settledOnly
                heading="Questions from the agent"
                headingRef={(el) => (questionsHeading = el)}
              />
            </section>
          </div>
        </Fold>
      </RunSection>

      <RunSection id="files" title="Files from this run" fold defaultOpen={!terminal()} summary={`${listed().length} ${listed().length === 1 ? 'file' : 'files'}.`}>
        <Show when={!artifacts.loading} fallback={<p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>Loading the files…</p>}>
          <ArtifactDownloadPanel requestId={props.requestId} attemptNumber={n()} artifacts={listed()} />
        </Show>
      </RunSection>

      <SideSlot page={props.page} target={props.sideTarget}>
        <div data-testid="attempt-side" class="min-w-0 space-y-4">
          <RunSection id="technical" title="Technical" summary="Ids, usage, raw log" fold>
            <Show when={terminal()}>
              <AgentGot requestId={props.requestId} attempt={props.attempt} workspace={folder()} />
              <Fold kind="run-usage" testId="attempt-usage-fold" size="sub" title="Usage" summary={usageLine()}>
                <UsagePanel attempt={props.attempt} />
              </Fold>
            </Show>
            <section data-testid="attempt-ids" class="space-y-1.5 text-xs">
              <h4 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>Ids</h4>
              <dl class="space-y-1">
                <IdRow label="Request" value={props.requestId} />
                <IdRow label="Attempt" value={props.attempt.attempt_id} />
                <IdRow label="Runner" value={props.attempt.runner_id} />
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
            <Show when={mrp.pack()}>{(pack) => <SignatureState signature={pack().signature} />}</Show>
            <Show when={terminal()}>
              <ArtifactView requestId={props.requestId} attemptNumber={n()} artifact={logOpen() ? log() : undefined} onClose={() => setLogOpen(false)} />
            </Show>
          </RunSection>
        </div>
      </SideSlot>
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

/** What a finished attempt used, in tokens: they are what the harness
 *  measured. The
 *  model provenance sits beside the model, and only when it says something
 *  the model name doesn't. */
const UsagePanel: Component<{ attempt: AttemptSummary }> = (props) => {
  const usage = () => readAttemptUsage(props.attempt);
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
  return (
    <section data-testid="attempt-usage" aria-label="Usage" class="space-y-2">
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
    </section>
  );
};


/**
 * The attempts made against one execution request, reading real data from
 * `store.ts#attemptsFor`/`loadAttempts`. A run with several attempts offers chips to pick
 * one, the newest by default, and shows only the one picked, read top-down: verdict, what needs
 * the person, checks, changes, steps, files, and the technical detail behind a fold.
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
                  profileId={props.profileId}
                  agentKind={props.agentKind}
                  runAgain={props.runAgain}
                  onReviewed={props.onReviewed}
                  nowShown={props.nowShown}
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
