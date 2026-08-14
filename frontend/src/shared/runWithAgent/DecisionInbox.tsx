import { type Component, For, Show, createSignal } from 'solid-js';
import { Badge, Button, Field } from '../ui';
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

export interface DecisionInboxProps {
  attemptId: string;
  /**
   * Known decisions to render as rows. Always an empty array in production
   * today — no decision-discovery endpoint exists anywhere in this
   * codebase (see `shared/execution/decisions.ts`'s header comment and
   * `docs/agent-handoffs/part-iii/III-F1.md`'s own "likely III-F4's
   * frontend-integration concern" note). This prop exists so the row
   * rendering — in particular pending-vs-expired-vs-resolved's distinct
   * visual treatment — is fully built and tested today, ready to receive
   * real rows the moment a list endpoint lands, without a caller needing to
   * change how it's mounted.
   */
  decisions?: DecisionRecord[];
  /** Called after any successful resolve, from a listed row or the manual
   *  quick action below the list. */
  onResolved?: (result: ResolveDecisionResult) => void;
}

/** Shared resolve call + user-facing toast for both `DecisionRow` and
 *  `ManualDecisionResolve` — one place maps every distinct server outcome
 *  to a distinct message, so the two entry points can never drift apart. */
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
        : 'Decision resolved.',
    );
    onResolved?.(result);
  } catch (err) {
    if (isDecisionTokenRejected(err)) {
      toast.error(
        'Rejected — either this deployment has not configured decision resolution ' +
          '(TACK_EXECUTION_DECISION_TOKEN), or the token entered above is wrong.',
      );
    } else if (isDecisionExpired(err)) {
      toast.error('This decision already expired — it can no longer be resolved.');
    } else if (isDecisionIdempotencyConflict(err)) {
      toast.error('Already resolved with a different answer than the one just submitted.');
    } else if (isDecisionNotFound(err)) {
      toast.error('No decision with that id exists for this attempt.');
    } else if (isDecisionInvalidOption(err)) {
      toast.error("That answer wasn't one of this decision's declared options.");
    } else {
      toast.error(err instanceof Error ? err.message : 'Failed to resolve the decision.');
    }
    throw err;
  }
}

const DecisionRow: Component<{
  attemptId: string;
  decision: DecisionRecord;
  onResolved?: (result: ResolveDecisionResult) => void;
}> = (props) => {
  const [optionId, setOptionId] = createSignal('');
  const [text, setText] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const isPending = () => props.decision.state === 'pending';
  const isExpired = () => props.decision.state === 'expired';
  const isResolved = () => props.decision.state === 'resolved';
  const hasOptions = () => props.decision.options.length > 0;

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
      class="space-y-2 rounded-lg border p-3"
      style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-light)' }}
    >
      <div class="flex flex-wrap items-center gap-2">
        {/* Pending / expired / resolved are visually AND semantically
            distinct — three different tones, three different labels, never
            merged into one generic "decision" badge (this card's acceptance
            bar names this explicitly). */}
        <Show when={isPending()}>
          <Badge tone="warning">Pending</Badge>
        </Show>
        <Show when={isExpired()}>
          <Badge tone="danger">Expired</Badge>
        </Show>
        <Show when={isResolved()}>
          <Badge tone="success">Resolved</Badge>
        </Show>
        <Show when={!isPending() && !isExpired() && !isResolved()}>
          <Badge tone="neutral">{props.decision.state} (unrecognised)</Badge>
        </Show>
        <span class="text-xs" style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
          {props.decision.decision_id}
        </span>
      </div>

      <p class="text-sm" style={{ color: 'var(--color-text-primary)' }}>
        {props.decision.prompt}
      </p>

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
        <form class="space-y-2" onSubmit={submit}>
          <Show
            when={hasOptions()}
            fallback={
              <Field
                label="Answer (option id)"
                value={optionId()}
                onInput={(e) => setOptionId(e.currentTarget.value)}
                required
                hint="This decision declared no fixed options — enter any non-empty answer id."
              />
            }
          >
            <fieldset class="space-y-1.5">
              <legend class="text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                Choose an answer
              </legend>
              <For each={props.decision.options}>
                {(opt) => (
                  <label class="flex items-center gap-1.5 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    <input
                      type="radio"
                      name={`decision-${props.decision.decision_id}`}
                      value={opt.option_id}
                      checked={optionId() === opt.option_id}
                      onChange={() => setOptionId(opt.option_id)}
                    />
                    {opt.label}
                  </label>
                )}
              </For>
            </fieldset>
          </Show>
          <Field
            label="Details (optional)"
            value={text()}
            onInput={(e) => setText(e.currentTarget.value)}
          />
          <div class="flex items-center gap-2">
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

const ManualDecisionResolve: Component<{
  attemptId: string;
  onResolved?: (result: ResolveDecisionResult) => void;
}> = (props) => {
  const [decisionId, setDecisionId] = createSignal('');
  const [optionId, setOptionId] = createSignal('');
  const [text, setText] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const canSubmit = () => decisionId().trim().length > 0 && optionId().trim().length > 0 && !busy();

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!canSubmit()) return;
    setBusy(true);
    try {
      await resolveAndNotify(
        props.attemptId,
        decisionId().trim(),
        { option_id: optionId().trim(), text: text().trim() || null },
        props.onResolved,
      );
      setDecisionId('');
      setOptionId('');
      setText('');
    } catch {
      /* already toasted */
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      class="space-y-2 rounded-lg border border-dashed p-3"
      style={{ 'border-color': 'var(--color-border-light)' }}
      onSubmit={submit}
    >
      <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
        Resolve a decision by id — no decision-discovery endpoint exists on this deployment yet
        (see docs/agent-handoffs/part-iii/III-F4.md), so enter an id you already know (e.g. from
        the runner/agent's own output).
      </p>
      <Field label="Decision id" value={decisionId()} onInput={(e) => setDecisionId(e.currentTarget.value)} required />
      <Field label="Answer (option id)" value={optionId()} onInput={(e) => setOptionId(e.currentTarget.value)} required />
      <Field label="Details (optional)" value={text()} onInput={(e) => setText(e.currentTarget.value)} />
      <div class="flex items-center gap-2">
        <Button type="submit" size="sm" variant="secondary" disabled={!canSubmit()} loading={busy()}>
          Resolve decision
        </Button>
        <Show when={!decisionId().trim() || !optionId().trim()}>
          <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            Enter a decision id and an answer to enable Resolve.
          </span>
        </Show>
      </div>
    </form>
  );
};

/**
 * The decision inbox for one attempt (TODO.md III-F4: "decision inbox").
 * Renders any known decisions with pending/expired/resolved kept visually
 * and semantically distinct (this card's acceptance bar, verbatim), and
 * always offers a manual "resolve by id" action that works against the
 * real, mounted `POST /attempts/{attempt_id}/decisions/{decision_id}/resolve`
 * endpoint today — see `props.decisions`' doc comment for why the list
 * itself is empty in every real deployment right now.
 *
 * The decision token field mirrors `features/approvals/ApprovalsPage.tsx`'s
 * own `TACK_ORCH_APPROVAL_TOKEN` entry exactly, including its reasoning:
 * always render the control and let a real resolve attempt's actual 403
 * answer "is this configured at all", rather than guessing client-side (see
 * `features/approvals/api.ts`'s `PendingApprovalListResponse` doc comment
 * for the fuller argument this mirrors).
 */
const DecisionInbox: Component<DecisionInboxProps> = (props) => {
  const decisions = () => props.decisions ?? [];
  const [tokenInput, setTokenInput] = createSignal(decisionTokenStore.get() ?? '');

  const saveToken = () => {
    decisionTokenStore.set(tokenInput().trim() || null);
    toast.success('Decision token saved for this browser session.');
  };

  return (
    <div class="space-y-3">
      <div
        class="flex flex-wrap items-end gap-2 rounded-lg p-2.5"
        style={{ border: '1px solid var(--color-border-light)', 'background-color': 'var(--color-bg-subtle)' }}
      >
        <Field
          label="Your decision token"
          type="password"
          autocomplete="off"
          placeholder="TACK_EXECUTION_DECISION_TOKEN"
          value={tokenInput()}
          onInput={(e) => setTokenInput(e.currentTarget.value)}
          hint="Stored only in this browser. Required to resolve — never sent when just viewing."
          class="min-w-56 flex-1"
        />
        <Button size="sm" variant="secondary" onClick={saveToken}>
          Save
        </Button>
      </div>

      <Show
        when={decisions().length > 0}
        fallback={
          <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            No decisions are listed here yet — see the quick action below if you already know a
            decision id.
          </p>
        }
      >
        <ul class="space-y-2">
          <For each={decisions()}>
            {(decision) => (
              <DecisionRow attemptId={props.attemptId} decision={decision} onResolved={props.onResolved} />
            )}
          </For>
        </ul>
      </Show>

      <ManualDecisionResolve attemptId={props.attemptId} onResolved={props.onResolved} />
    </div>
  );
};

export default DecisionInbox;
