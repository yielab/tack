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
  type AgentCapabilityId,
  type AgentCapabilityLine,
} from '../../shared/execution';
import type { RunnerSummary } from '../../shared/execution/api';
import { harnessesOf } from './runnerObservations';
import { CapabilityGlyph } from '../../shared/runWithAgent/runDialogIcons';
import DisabledLine from './DisabledLine';

/** The rows of the matrix, in the order `agentCapabilityLines` returns them. */
const AGENT_CAPABILITY_ROWS: readonly AgentCapabilityId[] = ['cancel', 'resume', 'decisions', 'proposals', 'usage'];

const MARK: Record<AgentCapabilityLine['state'], string> = { can: '✓', cannot: '✗', not_reported: '—' };

const capitalised = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/**
 * One cell of the matrix: a mark (the state is the glyph, never the colour alone), the whole line for a
 * screen reader, and the agent's own note behind "How we know". An agent no computer reports has no
 * note to give: its column header says so.
 */
const Cell: Component<{ kind: string; line: AgentCapabilityLine; noted: boolean }> = (props) => (
  <td
    data-testid={`cap-${props.line.id}`}
    data-agent={props.kind}
    data-state={props.line.state}
    aria-disabled={props.line.state === 'can' ? undefined : 'true'}
    class="min-w-0 px-2 py-2 align-top text-small"
    style={{ 'border-top': '1px solid var(--color-border-light)', 'overflow-wrap': 'anywhere' }}
  >
    <span
      aria-hidden="true"
      class="text-sm font-bold"
      style={{ color: props.line.state === 'can' ? 'var(--color-success-600)' : 'var(--color-text-tertiary)' }}
    >
      {MARK[props.line.state]}
    </span>
    <Show when={!props.noted}>
      <span class="sr-only">{props.line.text}</span>
    </Show>
    <Show when={props.noted}>
      <details data-testid={`how-we-know-${props.line.id}`}>
        <summary class="cursor-pointer text-caption" style={{ color: 'var(--color-text-secondary)' }}>
          <span class="sr-only">{props.line.text}. </span>
          How we know
        </summary>
        <p class="mt-1 text-caption" style={{ color: props.line.state === 'can' ? 'var(--color-text-primary)' : 'var(--color-text-tertiary)' }}>
          {props.line.text}
          <Show when={props.line.reason}>
            <span style={{ color: 'var(--color-text-tertiary)' }}> · {props.line.reason}</span>
          </Show>
        </p>
      </details>
    </Show>
  </td>
);

/**
 * What each agent can do, from what the agent itself reported, as a matrix: agents are the columns,
 * the five things an agent can do the rows. A thing it did not report is a dash and one it cannot do a
 * cross, each with its reason behind "How we know", never left out.
 */
const AgentCapabilities: Component<{ runners: readonly RunnerSummary[] }> = (props) => {
  const reports = () => props.runners.flatMap((r) => harnessesOf(r));
  const columns = () =>
    AGENT_KINDS.map((kind) => {
      const harness = pickHarness(reports(), kind);
      return { kind, harness, lines: agentCapabilityLines(kind, harness) };
    });
  return (
    <section class="flex min-w-0 flex-col gap-3 rounded-card bg-panel px-5 py-5" data-testid="agent-capabilities">
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
      <div class="overflow-x-auto">
        <table class="w-full table-fixed border-collapse text-left" style={{ 'min-width': '36rem' }}>
          <caption class="sr-only">What each agent can do. ✓ can, ✗ cannot, — not reported.</caption>
          <thead>
            <tr>
              <td style={{ width: '24%' }} />
              <For each={columns()}>
                {(col) => (
                  <th scope="col" data-testid={`agent-${col.kind}`} class="min-w-0 px-2 pb-2 align-top font-normal" style={{ 'overflow-wrap': 'anywhere' }}>
                    <span class="block text-sm font-bold" style={{ color: 'var(--color-text-primary)' }}>
                      {agentName(col.kind)}
                    </span>
                    <Show
                      when={col.harness}
                      fallback={
                        <DisabledLine>
                          No computer reports {agentName(col.kind)}. Install it and turn on agents on this computer to see what it can do.
                        </DisabledLine>
                      }
                    >
                      <p data-testid="agent-can" class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>
                        {agentCanSentence(col.kind, col.lines)}
                      </p>
                    </Show>
                  </th>
                )}
              </For>
            </tr>
          </thead>
          <tbody>
            <For each={AGENT_CAPABILITY_ROWS}>
              {(id, row) => (
                <tr>
                  <th scope="row" class="min-w-0 px-2 py-2 text-left align-top text-small font-normal" style={{ 'border-top': '1px solid var(--color-border-light)', color: 'var(--color-text-primary)' }}>
                    <span class="flex items-start gap-1.5">
                      <span class="mt-0.5" style={{ color: 'var(--color-text-secondary)' }}>
                        <CapabilityGlyph id={id} />
                      </span>
                      <span class="min-w-0" style={{ 'overflow-wrap': 'anywhere' }}>{capitalised(columns()[0].lines[row()].can)}</span>
                    </span>
                  </th>
                  <For each={columns()}>
                    {(col) => <Cell kind={col.kind} line={col.lines[row()]} noted={!!col.harness} />}
                  </For>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </section>
  );
};

export default AgentCapabilities;
