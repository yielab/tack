import { type Component, For, Show, createEffect, createResource, onCleanup, onMount } from 'solid-js';
import { onInboxChange } from '../state/notifications';
import { Badge, EmptyState, LoadError } from '../ui';
import { toast } from '../ui/toast';
import {
  decisionsApi,
  isDecisionExpired,
  isDecisionIdempotencyConflict,
  isDecisionInvalidOption,
  isDecisionNotFound,
  isDecisionTokenRejected,
  type DecisionAnswer,
  type DecisionRecord,
  type ResolveDecisionResult,
} from '../execution';
import DecisionTokenField from './DecisionTokenField';
import AnswerBox from './AnswerBox';
import { isTerminalAttemptState } from './attemptFormat';
import { truncateMiddle } from '../ui/truncateMiddle';

export interface DecisionInboxProps {
  requestId: string;
  attemptNumber: number;
  /** Internal attempt id — distinct from `attemptNumber` — the resolve
   *  route (`POST /attempts/{attempt_id}/decisions/{decision_id}/resolve`)
   *  is scoped by, matching every other caller of that route in this
   *  codebase. */
  attemptId: string;
  /** Called after any successful resolve. The list is refetched
   *  immediately after, so a resolved row's badge updates without the
   *  caller needing to do anything. */
  onResolved?: (result: ResolveDecisionResult) => void;
  /** The attempt's state. While it is not finished the list is read again when the inbox says a
   *  question of this run opened or closed, so one asked after the page opened appears; when it is
   *  finished, or absent, it is read once. */
  attemptState?: string;
  /** Only the questions still waiting for an answer, and nothing at all when there is none. */
  pendingOnly?: boolean;
  /** Only the questions already answered or expired, as a history: no token field (the questions
   *  that wait have their own place), and one muted line when the agent has asked nothing at all. */
  settledOnly?: boolean;
  /** A heading for the list, drawn only when there is something to list: an agent that asked nothing gets one grey line, not a heading over nothing. */
  heading?: string;
  headingRef?: (el: HTMLHeadingElement) => void;
}

/** Shared resolve call + user-facing toast — one place maps every distinct
 *  server outcome to a distinct message. */
async function resolveAndNotify(
  attemptId: string,
  decisionId: string,
  answer: DecisionAnswer,
  onResolved: ((result: ResolveDecisionResult) => void) | undefined,
): Promise<void> {
  try {
    const result = await decisionsApi.resolve(attemptId, decisionId, answer);
    toast.success(
      result.replayed
        ? 'Already resolved — this is the previously recorded answer.'
        : 'Answer sent.',
    );
    onResolved?.(result);
  } catch (err) {
    if (isDecisionTokenRejected(err)) {
      toast.error(
        'Rejected — either this board has no decision token set up, or the token entered above is wrong.',
      );
    } else if (isDecisionExpired(err)) {
      toast.error('This question already expired — it can no longer be answered.');
    } else if (isDecisionIdempotencyConflict(err)) {
      toast.error('Already resolved with a different answer than the one just submitted.');
    } else if (isDecisionNotFound(err)) {
      toast.error('That question no longer exists for this run.');
    } else if (isDecisionInvalidOption(err)) {
      toast.error("That answer wasn't one of the options offered.");
    } else {
      toast.error(err instanceof Error ? err.message : 'Could not send the answer.');
    }
    throw err;
  }
}

/** Whether a prompt is the whole text the agent was given (its card fenced in `<<<card-text id=…>>>`, or simply long) and not the question put to the person. */
function looksLikeDispatchPrompt(prompt: string): boolean {
  return /<<<[\w-]+\s+id=/.test(prompt) || prompt.length > 500 || prompt.split('\n').length > 10;
}

/** Copies text and says so; a refused clipboard says what the text was. */
async function copyId(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast.success('Decision id copied.');
  } catch {
    toast.error(`Couldn't copy the decision id: ${text}`);
  }
}

const DecisionRow: Component<{
  attemptId: string;
  decision: DecisionRecord;
  onResolved?: (result: ResolveDecisionResult) => void;
  onView: (decisionId: string) => void;
}> = (props) => {
  const isPending = () => props.decision.state === 'pending';
  const isExpired = () => props.decision.state === 'expired';
  const isResolved = () => props.decision.state === 'resolved';
  // The agent's own tool took the answer (docket, at its own CLI): nothing here can answer it.
  const isAnsweredElsewhere = () => props.decision.state === 'answered_elsewhere';
  const answeredIn = () => props.decision.resolved_by?.subject_id || 'the agent\'s own tool';
  // An approval of an action that leaves the team (an email, a post, a write to a service) is
  // marked by the runner, and carries the whole action while it is pending.
  const metadata = () => (props.decision.metadata && typeof props.decision.metadata === 'object' ? (props.decision.metadata as Record<string, unknown>) : {});
  const leaves = () => metadata().outward === true;
  const call = () => {
    const text = metadata().call;
    return typeof text === 'string' && text.trim() !== '' ? text : null;
  };
  // One of several questions the same step asked at once; the step waits for an answer to each.
  const part = () => {
    const raw = metadata().part as { index?: unknown; count?: unknown } | undefined;
    const index = typeof raw?.index === 'number' ? raw.index : null;
    const count = typeof raw?.count === 'number' ? raw.count : null;
    return index !== null && count !== null && count > 1 ? `Question ${index} of ${count}` : null;
  };
  // What needs approving, said first. The text the agent was given (a whole card, fenced) is not the
  // question: it waits in a closed fold, and the question is the call, or the line that says an approval is asked.
  const prompt = () => props.decision.prompt ?? '';
  const dispatch = () => looksLikeDispatchPrompt(prompt());
  const headline = () => (dispatch() ? 'The agent asks for your approval to go on.' : props.decision.prompt);

  onMount(() => {
    if (isPending() && !props.decision.viewed_at) props.onView(props.decision.decision_id);
  });

  const send = (answer: DecisionAnswer) =>
    resolveAndNotify(props.attemptId, props.decision.decision_id, answer, props.onResolved);

  return (
    <li
      class="space-y-3 p-4"
      style={{
        'border-radius': isPending() ? '26px' : 'var(--radius-item)',
        'background-color': isPending() ? 'var(--color-accent-soft)' : 'var(--color-bg-panel)',
      }}
    >
      <div class="flex flex-wrap items-center gap-2">
        {/* Pending / expired / resolved are visually AND semantically
            distinct — three different tones, three different labels, never
            merged into one generic "decision" badge. */}
        <Badge tone="neutral">{props.decision.kind}</Badge>
        <Show when={part()}>{(words) => <Badge tone="neutral">{words()}</Badge>}</Show>
        <Show when={isPending()}>
          <Badge tone="info">Pending</Badge>
        </Show>
        <Show when={isExpired()}>
          <Badge tone="neutral">Expired</Badge>
        </Show>
        <Show when={isResolved()}>
          <Badge tone="success">Resolved</Badge>
        </Show>
        <Show when={isAnsweredElsewhere()}>
          <Badge tone="success">Answered in {answeredIn()}</Badge>
        </Show>
        <Show when={!isPending() && !isExpired() && !isResolved() && !isAnsweredElsewhere()}>
          <Badge tone="neutral">{props.decision.state} (unrecognised)</Badge>
        </Show>
        <span class="ml-auto flex min-w-0 items-center gap-1.5 text-micro" style={{ color: 'var(--color-text-tertiary)' }}>
          <span data-testid="decision-id" title={props.decision.decision_id} style={{ 'font-family': 'var(--font-mono)' }}>
            {truncateMiddle(props.decision.decision_id ?? '')}
          </span>
          <button
            type="button"
            aria-label="Copy the decision id"
            class="min-h-6 min-w-6 flex-none px-1 font-semibold hover:underline"
            style={{ color: 'var(--color-accent-ink)' }}
            onClick={() => void copyId(props.decision.decision_id)}
          >
            Copy
          </button>
        </span>
      </div>

      <Show when={leaves()}>
        <h4 class="text-sm font-bold" style={{ color: 'var(--color-text-primary)' }}>
          This action leaves the team
        </h4>
      </Show>
      {/* What needs approving comes first: the action as it would be made, uncut (Tack keeps it only
          while this question is open; it is erased when the question is answered or expires), or the
          question itself. */}
      <Show
        when={isPending() && call()}
        fallback={
          <p data-testid="decision-headline" class="text-sm font-semibold" style={{ color: isPending() ? 'var(--color-accent-ink)' : 'var(--color-text-primary)' }}>
            {headline()}
          </p>
        }
      >
        <div class="space-y-1">
          <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            Exactly what happens if you approve:
          </p>
          <pre
            data-testid="decision-call"
            tabindex="0"
            aria-label="The action that would leave the team"
            class="overflow-auto whitespace-pre-wrap break-words rounded-item p-3 text-xs"
            style={{
              'font-family': 'var(--font-mono)',
              'line-height': '1.5',
              // Twenty lines at a line height of 1.5 show at once; a longer action scrolls.
              'max-height': '30em',
              'background-color': 'var(--color-bg-panel)',
              color: 'var(--color-text-primary)',
            }}
          >
            {call()}
          </pre>
        </div>
      </Show>
      <Show when={dispatch()}>
        <details data-testid="decision-given" class="min-w-0">
          <summary class="cursor-pointer text-xs font-semibold" style={{ color: 'var(--color-accent-ink)' }}>What the agent was given</summary>
          <pre
            tabindex="0"
            aria-label="What the agent was given"
            class="mt-2 overflow-auto whitespace-pre-wrap break-words rounded-item p-3 text-xs"
            style={{
              'font-family': 'var(--font-mono)',
              'line-height': '1.5',
              'max-height': '20em',
              'background-color': 'var(--color-bg-panel)',
              color: 'var(--color-text-primary)',
            }}
          >
            {prompt()}
          </pre>
        </details>
      </Show>

      <Show when={isResolved()}>
        <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          Answered: <span style={{ 'font-family': 'var(--font-mono)' }}>{props.decision.answer?.option_id}</span>
          {props.decision.answer?.text ? ` — "${props.decision.answer.text}"` : ''}
        </p>
      </Show>

      <Show when={isAnsweredElsewhere()}>
        <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          This was answered in {answeredIn()}, not here — nothing is left to resolve.
        </p>
      </Show>

      {/* Expired: the control is disabled and the reason is visible text
          right next to it, never a silently-greyed-out button (acceptance
          bar: "disabled controls name reason"). */}
      <Show when={isExpired()}>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          This decision expired without an answer — it can no longer be resolved.
        </p>
      </Show>

      <Show when={isPending()}>
        <AnswerBox decision={props.decision} onSend={send} />
      </Show>
    </li>
  );
};

/**
 * The decision inbox for one attempt. Fetches `GET
 * /executions/{request_id}/attempts/{attempt_number}/decisions` and renders
 * every row with pending/expired/resolved kept visually and semantically
 * distinct. A successful resolve refetches the list so the resolved row's
 * badge updates immediately, without the caller managing any state of its
 * own.
 *
 * The decision token field always renders the control and lets a real
 * resolve attempt's actual 403 answer "is this configured at all", rather
 * than guessing client-side.
 */
const DecisionInbox: Component<DecisionInboxProps> = (props) => {
  // A reload hands back the rows it already had, so a row being answered keeps what was typed in it.
  let previous: DecisionRecord[] = [];
  const unchanged = (a: DecisionRecord, b: DecisionRecord) =>
    JSON.stringify({ ...a, viewed_at: null }) === JSON.stringify({ ...b, viewed_at: null });
  const [decisions, { refetch }] = createResource<DecisionRecord[], string>(
    () => `${props.requestId}:${props.attemptNumber}`,
    async (_key, { refetching }) => {
      // A reload that fails keeps the questions already on screen; only the first read reports an error.
      const fresh = await decisionsApi.list(props.requestId, props.attemptNumber).catch((err) => {
        if (refetching && previous.length > 0) return previous;
        throw err;
      });
      const merged = fresh.map((row) => {
        const prior = previous.find((p) => p.decision_id === row.decision_id);
        return prior && unchanged(prior, row) ? prior : row;
      });
      previous = merged;
      return merged;
    },
  );
  const shown = () => {
    const all = decisions.error ? [] : (decisions.latest ?? []);
    if (props.pendingOnly) return all.filter((d) => d.state === 'pending');
    return props.settledOnly ? all.filter((d) => d.state !== 'pending') : all;
  };
  const askedNothing = () => !firstLoad() && !decisions.error && (decisions.latest ?? []).length === 0;
  const firstLoad = () => decisions.loading && decisions.latest === undefined;

  createEffect(() => {
    if (props.attemptState === undefined || isTerminalAttemptState(props.attemptState)) return;
    // A change of another run's, or of a kind that is not a question, leaves this list as it is.
    // A socket that reconnected may have missed one, so that reads it again too.
    const stop = onInboxChange((change) => {
      if (change === 'resync' || (change.kind === 'decision' && change.requestId === props.requestId)) void refetch();
    });
    onCleanup(stop);
  });

  // One `viewed` call per decision id for as long as the inbox is mounted:
  // a refetch hands every row a new object and remounts it, and must not
  // call again.
  const viewed = new Set<string>();
  const markViewed = (decisionId: string) => {
    if (viewed.has(decisionId)) return;
    viewed.add(decisionId);
    decisionsApi.markViewed(props.attemptId, decisionId).catch(() => {
      /* best effort: the badge it feeds is not worth a toast */
    });
  };

  const handleResolved = (result: ResolveDecisionResult) => {
    props.onResolved?.(result);
    void refetch();
  };

  return (
    <div class="space-y-3">
      <Show when={firstLoad() && !props.pendingOnly && !props.settledOnly}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading decisions…
        </p>
      </Show>
      <Show when={decisions.error}>
        <LoadError what="the questions" error={decisions.error} onRetry={() => void refetch()} />
      </Show>
      <Show when={!firstLoad() && !decisions.error && !props.pendingOnly && !props.settledOnly && shown().length === 0}>
        <EmptyState title="The agent has asked nothing yet" />
      </Show>
      <Show when={props.heading && shown().length > 0}>
        <h5 ref={props.headingRef} tabIndex={-1} class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          {props.heading}
        </h5>
      </Show>
      <Show when={props.settledOnly && askedNothing()}>
        <p data-testid="asked-nothing" class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          The agent has asked nothing yet
        </p>
      </Show>
      <Show when={shown().length > 0}>
        <ul class="space-y-2">
          <For each={shown()}>
            {(decision) => (
              <DecisionRow attemptId={props.attemptId} decision={decision} onResolved={handleResolved} onView={markViewed} />
            )}
          </For>
        </ul>
      </Show>

      <Show when={!props.settledOnly && (!props.pendingOnly || shown().length > 0)}>
        <div
          class="rounded-item px-4 py-3"
          style={{ 'background-color': 'var(--color-bg-panel)' }}
        >
          <DecisionTokenField />
        </div>
      </Show>
    </div>
  );
};

export default DecisionInbox;
