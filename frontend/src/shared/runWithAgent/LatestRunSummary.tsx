import { type Component, Show, createEffect, createMemo, onCleanup, onMount } from 'solid-js';
import { Badge, Button } from '../ui';
import { useExecutionStore } from '../state/executionContext';
import type { AttemptSummary } from '../execution';
import {
  describeAttemptStatus,
  formatApproxCost,
  formatTokenCount,
  isTerminalAttemptState,
  readAttemptUsage,
  NOT_MEASURED_TEXT,
} from './attemptFormat';
import { describeExecutionState, relativeTimeFromIso } from './shared';

export interface LatestRunSummaryProps {
  itemId: string;
  /** Opens the run's full report (the item's Execution tab). */
  onOpen: () => void;
}

const EXCERPT_CHARS = 220;

/** The item's latest run in two lines — its status, what the agent said and
 *  what it used — so coming back to a task shows what happened without
 *  opening the Execution tab. Renders nothing until the item has a run. */
const LatestRunSummary: Component<LatestRunSummaryProps> = (props) => {
  const store = useExecutionStore();
  onMount(() => onCleanup(store.watchItem(props.itemId)));
  const latest = createMemo(() => {
    const record = store.requestsForItem(props.itemId)[0];
    return record?.status === 'ready' ? record.summary : undefined;
  });
  const attempts = createMemo(() => store.attemptsFor(latest()?.request_id ?? ''));
  createEffect(() => {
    const id = latest()?.request_id;
    if (id && attempts().status === 'idle') void store.loadAttempts(id).catch(() => undefined);
  });
  const attempt = createMemo<AttemptSummary | undefined>(() => {
    const a = attempts();
    if (a.status !== 'ready') return undefined;
    return a.data.reduce<AttemptSummary | undefined>((max, x) => (!max || x.attempt_number > max.attempt_number ? x : max), undefined);
  });
  const status = () => {
    const a = attempt();
    if (a) return describeAttemptStatus(a, a.review);
    const s = describeExecutionState(latest()?.state ?? 'queued');
    return { label: s.label, tone: s.tone };
  };
  const finished = () => {
    const a = attempt();
    return !!a && isTerminalAttemptState(a.state);
  };
  const said = () => {
    const result = (attempt()?.terminal_reason as { result?: unknown } | null)?.result;
    if (typeof result !== 'string' || result.trim() === '') return null;
    const text = result.trim().replace(/\s+/g, ' ');
    return text.length > EXCERPT_CHARS ? `${text.slice(0, EXCERPT_CHARS - 1)}…` : text;
  };
  const usageLine = () => {
    const a = attempt();
    if (!a || !finished()) return null;
    const u = readAttemptUsage(a);
    const parts: string[] = [];
    if (u.tokensIn !== null || u.tokensOut !== null) {
      const total = (u.tokensIn ?? 0) + (u.tokensOut ?? 0);
      parts.push(`${formatTokenCount(total)} tokens`);
    }
    const cost = formatApproxCost(u.cost);
    if (cost !== NOT_MEASURED_TEXT) parts.push(cost);
    if (u.modelCalls !== null) parts.push(`${u.modelCalls} model ${u.modelCalls === 1 ? 'call' : 'calls'}`);
    if (u.models.length > 0) parts.push(u.models[0]);
    return parts.length > 0 ? parts.join(' · ') : null;
  };
  const working = () => {
    const state = attempt()?.state ?? latest()?.state;
    if (state === 'waiting_decision') return 'The agent is waiting for your answer.';
    if (state === 'queued') return 'Waiting for a runner to pick it up.';
    return 'The agent is working on it.';
  };

  return (
    <Show when={latest()}>
      {(run) => (
        <div data-testid="latest-run" class="min-w-0 flex-1 space-y-1.5">
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>Last run</span>
            <Badge tone={status().tone}>{status().label}</Badge>
            <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              {relativeTimeFromIso(attempt()?.ended_at ?? attempt()?.started_at ?? run().created_at)}
            </span>
          </div>
          <Show
            when={finished()}
            fallback={<p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>{working()}</p>}
          >
            <Show when={said()}>
              {(text) => (
                <p data-testid="latest-run-said" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                  “{text()}”
                </p>
              )}
            </Show>
          </Show>
          <Show when={usageLine()}>
            {(line) => (
              <p data-testid="latest-run-usage" class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                {line()}
              </p>
            )}
          </Show>
          <div class="pt-1">
            <Button size="sm" variant="secondary" onClick={() => props.onOpen()}>
              {finished() ? 'See what the agent did' : 'Follow the run'}
            </Button>
          </div>
        </div>
      )}
    </Show>
  );
};

export default LatestRunSummary;
