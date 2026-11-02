import { type Component, Show, createResource } from 'solid-js';
import RunState from '../../shared/ui/RunState';
import { attemptsApi, executionsApi } from '../../shared/execution';
import DecisionInbox from '../../shared/runWithAgent/DecisionInbox';
import { relativeTimeFromIso } from '../../shared/runWithAgent/shared';
import type { Turn } from '../../shared/api/threads';

/** The newest attempt of a run that is waiting for a person's answer, or null when the run is not. */
async function askingAttempt(requestId: string) {
  try {
    const { data: run } = await executionsApi.get(requestId);
    if (run.state !== 'waiting_decision') return null;
    const { data } = await attemptsApi.list(requestId);
    const attempts = data.data ?? [];
    return attempts.length > 0 ? attempts.reduce((a, b) => (b.attempt_number > a.attempt_number ? b : a)) : null;
  } catch {
    return null;
  }
}

/**
 * Where a turn is, under the message that started it: work in progress while the agent answers (with
 * its question, when it asks one, to be answered here) and, when it failed, that it failed. A turn that
 * replied needs no line of its own: the reply is the next message.
 */
const TurnState: Component<{ turn: Turn; version: unknown; onAnswered?: () => void }> = (props) => {
  const [asking] = createResource(
    () => (props.turn.state === 'pending' ? ([props.turn.request_id, props.version] as const) : false),
    ([requestId]) => askingAttempt(requestId),
  );
  return (
    <div
      data-testid="turn-state"
      data-state={props.turn.state}
      class="flex min-w-0 flex-col gap-2 pl-3"
      style={{ color: 'var(--color-text-secondary)' }}
    >
      <Show when={props.turn.state === 'pending'}>
        <p class="flex flex-wrap items-center gap-2 text-sm" role="status" aria-live="polite">
          <RunState state={asking() ? 'needs' : 'running'} />
          {asking() ? 'The agent is asking you something before it can answer.' : 'The agent is working on your message.'}
          <span class="text-xs">Started {relativeTimeFromIso(props.turn.started_at)}.</span>
        </p>
        <Show when={asking()}>
          {(attempt) => (
            <DecisionInbox
              requestId={attempt().request_id}
              attemptNumber={attempt().attempt_number}
              attemptId={attempt().attempt_id}
              attemptState={attempt().state}
              pendingOnly
              onResolved={() => props.onAnswered?.()}
            />
          )}
        </Show>
      </Show>
      <Show when={props.turn.state === 'failed'}>
        <p class="flex flex-wrap items-center gap-2 text-sm" role="status">
          <RunState state="failed" />
          The agent could not answer this message.
        </p>
      </Show>
    </div>
  );
};

export default TurnState;
