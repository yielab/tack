import { type Component, For, Show, createEffect, createResource, createSignal, onCleanup, onMount } from 'solid-js';
import { Badge, Button, EmptyState, Field, LoadError } from '../ui';
import { HELP } from '../help/texts';
import { toast } from '../ui/toast';
import {
  decisionTokenStore,
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
import RadioRow from './RadioRow';
import { isTerminalAttemptState } from './attemptFormat';

/** How often a question asked after the page opened is looked for, while the attempt is not over. */
const LIVE_RELOAD_MS = 3000;

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
  /** The attempt's state. While it is not finished the list is read again every few seconds, so a
   *  question asked after the page opened appears; when it is finished, or absent, it is read once. */
  attemptState?: string;
  /** Only the questions still waiting for an answer, and nothing at all when there is none. */
  pendingOnly?: boolean;
  /** Only the questions already answered or expired, as a history: no token field (the questions
   *  that wait have their own place), and one muted line when the agent has asked nothing at all. */
  settledOnly?: boolean;
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

const DecisionRow: Component<{
  attemptId: string;
  decision: DecisionRecord;
  onResolved?: (result: ResolveDecisionResult) => void;
  onView: (decisionId: string) => void;
}> = (props) => {
  const recommendedId = () => {
    const id = props.decision.recommendation?.option_id;
    return id && props.decision.options.some((o) => o.option_id === id) ? id : '';
  };
  const [optionId, setOptionId] = createSignal(recommendedId());
  const [text, setText] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const isPending = () => props.decision.state === 'pending';
  const isExpired = () => props.decision.state === 'expired';
  const isResolved = () => props.decision.state === 'resolved';
  const hasOptions = () => props.decision.options.length > 0;
  // An approval of an action that leaves the team (an email, a post, a write to a service) is
  // marked by the runner, and carries the whole action while it is pending.
  const metadata = () => (props.decision.metadata && typeof props.decision.metadata === 'object' ? (props.decision.metadata as Record<string, unknown>) : {});
  const leaves = () => metadata().outward === true;
  const call = () => {
    const text = metadata().call;
    return typeof text === 'string' && text.trim() !== '' ? text : null;
  };

  onMount(() => {
    if (isPending() && !props.decision.viewed_at) props.onView(props.decision.decision_id);
  });

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!optionId().trim() || busy()) return;
    setBusy(true);
    try {
      await resolveAndNotify(
        props.attemptId,
        props.decision.decision_id,
        { option_id: optionId().trim(), text: text().trim() || null },
        props.onResolved,
      );
    } catch {
      /* already toasted by resolveAndNotify */
    } finally {
      setBusy(false);
    }
  };

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
        <Show when={isPending()}>
          <Badge tone="info">Pending</Badge>
        </Show>
        <Show when={isExpired()}>
          <Badge tone="neutral">Expired</Badge>
        </Show>
        <Show when={isResolved()}>
          <Badge tone="success">Resolved</Badge>
        </Show>
        <Show when={!isPending() && !isExpired() && !isResolved()}>
          <Badge tone="neutral">{props.decision.state} (unrecognised)</Badge>
        </Show>
        <span class="ml-auto text-micro" style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
          {props.decision.decision_id}
        </span>
      </div>

      <Show when={leaves()}>
        <h4 class="text-sm font-bold" style={{ color: 'var(--color-text-primary)' }}>
          This action leaves the team
        </h4>
      </Show>
      <p class="text-sm font-semibold" style={{ color: isPending() ? 'var(--color-accent-ink)' : 'var(--color-text-primary)' }}>
        {props.decision.prompt}
      </p>

      {/* The action as it would be made, uncut. Tack keeps it only while this question is
          open; it is erased when the question is answered or expires. */}
      <Show when={isPending() && call()}>
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

      <Show when={isResolved()}>
        <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          Answered: <span style={{ 'font-family': 'var(--font-mono)' }}>{props.decision.answer?.option_id}</span>
          {props.decision.answer?.text ? ` — "${props.decision.answer.text}"` : ''}
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
        <form class="space-y-3" onSubmit={submit}>
          <Show
            when={hasOptions()}
            fallback={
              <Field
                label="Answer (option id)"
                value={optionId()}
                onInput={(e) => setOptionId(e.currentTarget.value)}
                required
                hint="This question has no fixed options — type any answer."
              />
            }
          >
            <fieldset class="flex flex-col gap-2">
              <legend class="mb-2 text-micro font-bold uppercase tracking-[0.1em]" style={{ color: 'var(--color-accent-ink)' }}>
                Choose an answer
              </legend>
              <For each={props.decision.options}>
                {(opt) => (
                  <RadioRow
                    name={`decision-${props.decision.decision_id}`}
                    value={opt.option_id}
                    checked={optionId() === opt.option_id}
                    onChange={() => setOptionId(opt.option_id)}
                  >
                    <span class="font-semibold">{opt.label}</span>
                    <Show when={opt.option_id === recommendedId()}>
                      <span class="ml-2 text-micro font-bold uppercase tracking-[0.1em]" style={{ color: 'var(--color-accent-ink)' }}>
                        Recommended
                      </span>
                    </Show>
                    <Show when={opt.description}>
                      <span class="block text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                        {opt.description}
                      </span>
                    </Show>
                    <Show when={opt.risks && opt.risks.length > 0}>
                      <span class="block text-xs" style={{ color: 'var(--color-danger-600)' }}>
                        Risks: {opt.risks!.join('; ')}
                      </span>
                    </Show>
                    <Show when={opt.estimated_tokens != null}>
                      <span class="block text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                        About {opt.estimated_tokens!.toLocaleString('en-US')} tokens
                      </span>
                    </Show>
                  </RadioRow>
                )}
              </For>
            </fieldset>
            <Show when={recommendedId()}>
              <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                Recommended: <span style={{ 'font-family': 'var(--font-mono)' }}>{recommendedId()}</span>
                <Show when={props.decision.recommendation?.rationale}>
                  {' — '}
                  {props.decision.recommendation?.rationale}
                </Show>
                <Show when={(props.decision.recommendation?.evidence_refs ?? []).length > 0}>
                  {' '}
                  (evidence: {props.decision.recommendation?.evidence_refs?.join(', ')})
                </Show>
              </p>
            </Show>
          </Show>
          <Field
            label="Details (optional)"
            value={text()}
            onInput={(e) => setText(e.currentTarget.value)}
          />
          <div class="flex flex-wrap items-center gap-2">
            <Button type="submit" size="sm" disabled={busy() || !optionId().trim()} loading={busy()}>
              Resolve
            </Button>
            <Show when={!optionId().trim()}>
              <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                Select or enter an answer to enable Resolve.
              </span>
            </Show>
          </div>
        </form>
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
  const [tokenInput, setTokenInput] = createSignal(decisionTokenStore.get() ?? '');
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
    const timer = setInterval(() => void refetch(), LIVE_RELOAD_MS);
    onCleanup(() => clearInterval(timer));
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

  const saveToken = () => {
    decisionTokenStore.set(tokenInput().trim() || null);
    toast.success('Decision token saved for this browser session.');
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
          class="flex flex-wrap items-end gap-2 rounded-item px-4 py-3"
          style={{ 'background-color': 'var(--color-bg-panel)' }}
        >
          <Field
            label="Your decision token"
            type="password"
            autocomplete="off"
            value={tokenInput()}
            onInput={(e) => setTokenInput(e.currentTarget.value)}
            hint="Ask your administrator for it. Kept only for this browser session."
            help={HELP.decisionToken}
            class="min-w-56 flex-1"
          />
          <Button size="sm" variant="secondary" onClick={saveToken}>
            Save
          </Button>
        </div>
      </Show>
    </div>
  );
};

export default DecisionInbox;
