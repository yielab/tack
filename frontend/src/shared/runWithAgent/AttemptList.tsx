import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button } from '../ui';
import { artifactsApi, attemptsApi, type AttemptSummary } from '../execution';
import { ITEM_UPDATED_EVENT } from '../state/itemEvents';
import {
  NOT_MEASURED_TEXT,
  describeAttemptOutcome,
  describeModelProvenance,
  formatUsageEconomics,
  isTerminalAttemptState,
} from './attemptFormat';
import ArtifactDownloadPanel from './ArtifactDownloadPanel';
import DecisionInbox from './DecisionInbox';
import EventTimeline from './EventTimeline';
import MrpPanel from './MrpPanel';
import { describeExecutionState, describePullRequestState, relativeTimeFromIso } from './shared';

export interface AttemptListProps {
  requestId: string;
  attempts: AttemptSummary[];
}

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

const AttemptRow: Component<{ requestId: string; attempt: AttemptSummary }> = (props) => {
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
    <li class="space-y-4 rounded-[20px] p-4" style={{ 'background-color': 'var(--color-bg-app)', 'box-shadow': 'var(--shadow-sm)' }}>
      <div class="flex flex-wrap items-center gap-2">
        <span class="font-heading text-lg" style={{ color: 'var(--color-text-primary)' }}>
          Attempt #{props.attempt.attempt_number}
        </span>
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
        <span class="ml-auto text-[11px]" style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
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
        <CostTile label="Model/token cost" value={economics().modelTokenCostUsd} />
        <CostTile label="Runner time" value={economics().runnerTime.wallClock} />
        <CostTile label="Runner time cost" value={economics().runnerTime.costUsd} />
      </dl>

      <Button size="sm" variant="ghost" onClick={() => setExpanded((v) => !v)} aria-expanded={expanded()}>
        {expanded() ? 'Hide details' : 'Show events, decisions & artifacts'}
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
              Decisions
            </h4>
            <DecisionInbox
              requestId={props.requestId}
              attemptNumber={props.attempt.attempt_number}
              attemptId={props.attempt.attempt_id}
            />
          </section>

          <section>
            <h4 class="mb-2 text-base" style={{ color: 'var(--color-text-primary)' }}>
              Artifacts
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
    <For each={props.attempts}>{(attempt) => <AttemptRow requestId={props.requestId} attempt={attempt} />}</For>
  </ul>
);

export default AttemptList;
