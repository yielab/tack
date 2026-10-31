import { type Component, For, Show } from 'solid-js';
import { HelpHint } from '../../shared/ui';
import { HELP } from '../../shared/help/texts';
import {
  AGENT_KINDS,
  PER_TASK_AGENT_TEXT,
  agentCanSentence,
  agentCapabilityLines,
  agentName,
  pickHarness,
  type AgentCapabilityLine,
} from '../../shared/execution';
import type { RunnerSummary } from '../../shared/execution/api';
import { harnessesOf } from './runnerObservations';
import DisabledLine from './DisabledLine';

const Line: Component<{ line: AgentCapabilityLine }> = (props) => (
  <li
    data-testid={`cap-${props.line.id}`}
    aria-disabled={props.line.state === 'can' ? undefined : 'true'}
    class="flex items-start gap-2 text-small"
    style={{ color: props.line.state === 'can' ? 'var(--color-text-primary)' : 'var(--color-text-tertiary)' }}
  >
    <span
      aria-hidden="true"
      class="mt-2 h-1.5 w-1.5 flex-none rounded-full"
      style={{ background: props.line.state === 'can' ? 'var(--color-success-600)' : 'var(--color-border-medium)' }}
    />
    <span class="min-w-0">
      {props.line.text}
      <Show when={props.line.reason}>
        <span style={{ color: 'var(--color-text-tertiary)' }}> · {props.line.reason}</span>
      </Show>
    </span>
  </li>
);

/**
 * What each agent can do, from what the agent itself reported. A thing it did not report, or cannot
 * do, is a muted line with its reason, never left out.
 */
const AgentCapabilities: Component<{ runners: readonly RunnerSummary[] }> = (props) => {
  const reports = () => props.runners.flatMap((r) => harnessesOf(r));
  return (
    <section class="flex flex-col gap-3 rounded-card bg-panel px-5 py-5" data-testid="agent-capabilities">
      <div>
        <div class="flex items-center gap-2">
          <h2 class="text-lead font-bold leading-tight" style={{ color: 'var(--color-text-primary)' }}>
            What each agent can do
          </h2>
          <HelpHint label="What each agent can do" help={HELP.agentCapabilities} />
        </div>
        <p class="mt-1 text-small" style={{ color: 'var(--color-text-secondary)' }}>
          {PER_TASK_AGENT_TEXT} What follows is what each agent told Tack about itself.
        </p>
      </div>
      <div class="grid gap-2.5 sm:grid-cols-2">
        <For each={AGENT_KINDS}>
          {(kind) => {
            const harness = () => pickHarness(reports(), kind);
            const lines = () => agentCapabilityLines(kind, harness());
            return (
              <div class="flex flex-col gap-1.5 rounded-xl bg-app px-3.5 py-3" data-testid={`agent-${kind}`}>
                <span class="text-sm font-bold" style={{ color: 'var(--color-text-primary)' }}>
                  {agentName(kind)}
                </span>
                <Show
                  when={harness()}
                  fallback={
                    <DisabledLine>
                      No computer reports {agentName(kind)}. Install it and turn on agents on this computer to see what it can do.
                    </DisabledLine>
                  }
                >
                  <p data-testid="agent-can" class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
                    {agentCanSentence(kind, lines())}
                  </p>
                  <ul class="flex flex-col gap-1">
                    <For each={lines()}>{(line) => <Line line={line} />}</For>
                  </ul>
                </Show>
              </div>
            );
          }}
        </For>
      </div>
    </section>
  );
};

export default AgentCapabilities;
