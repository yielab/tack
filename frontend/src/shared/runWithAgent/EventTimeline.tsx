import { type Component, For, Show, createEffect, createResource, createSignal, on, onCleanup } from 'solid-js';
import { Badge, EmptyState, LoadError } from '../ui';
import { attemptsApi, type EventSummary } from '../execution';
import { relativeTimeFromIso } from './shared';
import { NEEDS_A_PERSON_TEXT, inProject } from './attemptFormat';

export interface EventTimelineProps {
  requestId: string;
  attemptNumber: number;
  /** The attempt is still going: the timeline reloads every few seconds, and
   *  once more when it ends, so the agent's steps appear as it takes them. */
  live?: boolean;
  /** The runner's workspace: its prefix is left off every path an event names. */
  workspace?: string | null;
}

/** How often a live attempt's timeline reloads. */
const LIVE_RELOAD_MS = 3000;

/** Best-effort, human-readable rendering of an event's free-form `payload`
 *  (III.1.6: event `kind`/`payload` are harness/runner-defined, not a fixed
 *  schema — `docs/contracts/runner-v1/event-batch.request.json`'s own
 *  frozen example is just `{"stream": "summary", "text": "..."}`-shaped).
 *  Prefers a `text`/`message` string field when present (the common case in
 *  the frozen fixture), else falls back to compact JSON — never throws on
 *  an unexpected shape. */
function describeEventPayload(payload: unknown): string {
  if (payload === null || payload === undefined) return '';
  if (typeof payload === 'string') return payload;
  if (typeof payload === 'object') {
    const obj = payload as Record<string, unknown>;
    if (typeof obj.text === 'string') return obj.text;
    if (typeof obj.message === 'string') return obj.message;
    if (typeof obj.reason === 'string') return obj.reason;
    try {
      return JSON.stringify(payload);
    } catch {
      return '(unrenderable payload)';
    }
  }
  return String(payload);
}

/** Each runner event kind in words; an unknown kind shows as reported. */
const EVENT_LABELS: Record<string, { label: string; failed?: boolean }> = {
  'attempt.terminal': { label: 'The run ended' },
  'attempt.cancelled': { label: 'Cancelled' },
  'attempt.verify_skipped': { label: 'Verification skipped' },
  'attempt.verify_failed': { label: 'Verification failed', failed: true },
  'attempt.install_skipped': { label: 'Install skipped' },
  'attempt.push_skipped': { label: 'Branch not pushed' },
  'attempt.push_failed': { label: 'Pushing the branch failed', failed: true },
  'attempt.evidence_failed': { label: 'Evidence could not be collected', failed: true },
  'attempt.plan_invalid': { label: 'The plan could not be read', failed: true },
  'agent.model_turn': { label: 'A call to the model' },
  'agent.tool_call': { label: 'The agent ran a tool' },
  'agent.tool_refused': { label: 'A tool call was refused', failed: true },
  'agent.hop_started': { label: 'A step began' },
  'agent.hop_finished': { label: 'A step ended' },
  'agent.untracked': { label: 'Files left out of the evidence' },
  'agent.refused': { label: 'docket refused the task', failed: true },
  'agent.answer_failed': { label: 'docket did not take the answer', failed: true },
  'escalation.asked': { label: 'docket asked a person' },
  'escalation.resolved': { label: 'A person answered' },
  'escalation.expired': { label: 'Nobody answered before it expired' },
  'escalation.decided': { label: 'docket decided on its own' },
  'escalation.unreadable': { label: 'docket asked for something Tack cannot put to a person', failed: true },
};

/** `attempt.terminal`: the agent's message and files are on the card above, so this
 *  says only how it ended; the whole payload sits behind "Raw". */
const TerminalBody: Component<{ payload: Record<string, unknown> }> = (props) => {
  const [raw, setRaw] = createSignal(false);
  const how = () => {
    const p = props.payload;
    const text = typeof p.message === 'string' ? p.message : p.code === 'needs_human' ? NEEDS_A_PERSON_TEXT : typeof p.code === 'string' ? p.code : null;
    return text ?? 'See what the agent did and said above.';
  };
  return (
    <div class="mt-0.5 text-xs break-words" style={{ color: 'var(--color-text-secondary)' }}>
      <p>{how()}</p>
      <button type="button" class="underline" aria-expanded={raw()} onClick={() => setRaw((v) => !v)}>
        Raw
      </button>
      <Show when={raw()}>
        <pre class="max-h-64 overflow-auto whitespace-pre-wrap">{JSON.stringify(props.payload, null, 2)}</pre>
      </Show>
    </div>
  );
};

const EventRow: Component<{ event: EventSummary; last: boolean; workspace?: string | null }> = (props) => {
  const known = () => EVENT_LABELS[props.event.kind];
  return (
  <li class="grid grid-cols-[14px_minmax(0,1fr)_auto] gap-3">
    {/* Rail: a dot per event, joined by a line to the next one. */}
    <div class="flex flex-col items-center" aria-hidden="true">
      <span class="mt-1 h-3 w-3 flex-none rounded-full" style={{ 'background-color': known()?.failed ? 'var(--color-danger-600)' : 'var(--color-primary-600)' }} />
      <Show when={!props.last}>
        <span class="my-1 w-0.5 flex-1" style={{ 'background-color': 'var(--color-border-light)' }} />
      </Show>
    </div>
    <div class="min-w-0 pb-3.5">
      <div class="flex flex-wrap items-center gap-1.5">
        <span class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }} title={props.event.kind}>
          {known()?.label ?? props.event.kind}
        </span>
        <Badge tone="neutral">{props.event.source}</Badge>
      </div>
      <Show
        when={props.event.kind === 'attempt.terminal' && props.event.payload && typeof props.event.payload === 'object'}
        fallback={
          <p class="mt-0.5 text-xs break-words" style={{ color: 'var(--color-text-secondary)' }}>
            {inProject(describeEventPayload(props.event.payload), props.workspace)}
          </p>
        }
      >
        <TerminalBody payload={props.event.payload as Record<string, unknown>} />
      </Show>
    </div>
    <div class="flex flex-col items-end text-[0.6875rem]" style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
      <span>{relativeTimeFromIso(props.event.occurred_at)}</span>
    </div>
  </li>
  );
};

/**
 * The normalized event timeline for one attempt, reading
 * `GET /executions/{id}/attempts/{n}/events`. Oldest first, matching the
 * handler's own documented ordering; renders every fetch outcome
 * (loading/empty/error) explicitly rather than papering over a gap the way
 * `ExecutionTimeline.tsx` once had to for the whole attempts surface.
 */
const EventTimeline: Component<EventTimelineProps> = (props) => {
  const [events, { refetch }] = createResource(
    () => `${props.requestId}:${props.attemptNumber}`,
    () => attemptsApi.events(props.requestId, props.attemptNumber).then((r) => r.data.data),
  );
  createEffect(() => {
    if (!props.live) return;
    const timer = setInterval(() => void refetch(), LIVE_RELOAD_MS);
    onCleanup(() => clearInterval(timer));
  });
  createEffect(
    on(
      () => props.live,
      (live, wasLive) => {
        if (wasLive && !live) void refetch();
      },
      { defer: true },
    ),
  );
  // A reload keeps the rows on screen: only the first load says "Loading".
  const loaded = () => (events.state === 'ready' || events.state === 'refreshing' ? (events.latest ?? []) : null);

  return (
    <div>
      <Show when={events.state === 'pending'}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading events…
        </p>
      </Show>
      <Show when={events.state === 'errored'}>
        <LoadError what="the events" error={events.error} onRetry={() => void refetch()} />
      </Show>
      <Show when={loaded()?.length === 0}>
        <EmptyState title="No events reported yet" />
      </Show>
      <Show when={(loaded()?.length ?? 0) > 0}>
        <ul>
          <For each={loaded()}>
            {(event, i) => <EventRow event={event} last={i() === (loaded() ?? []).length - 1} workspace={props.workspace} />}
          </For>
        </ul>
      </Show>
    </div>
  );
};

export default EventTimeline;
