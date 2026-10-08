import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button } from '../ui';
import { artifactsApi, type AttemptSummary } from '../execution';
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
  const economics = () =>
    formatUsageEconomics(props.attempt.usage_economics, isTerminalAttemptState(props.attempt.state));

  return (
    <li class="space-y-4 rounded-[20px] p-4" style={{ 'background-color': 'var(--color-bg-app)', 'box-shadow': 'var(--shadow-sm)' }}>
      <div class="flex flex-wrap items-center gap-2">
        <span class="font-heading text-lg" style={{ color: 'var(--color-text-primary)' }}>
          Attempt #{props.attempt.attempt_number}
        </span>
        <Badge tone={outcome().badge?.tone ?? stateInfo().tone}>{outcome().badge?.label ?? stateInfo().label}</Badge>
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
