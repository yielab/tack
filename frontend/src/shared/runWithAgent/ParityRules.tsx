import { type Component, For, Show } from 'solid-js';
import type { HarnessCapability, ParityLine } from '../execution/types';

const BY_DESIGN = 'by design:';

/** Who applies a rule, in words. A gap says why in the runner's own words, without the card that closes it. */
function whoApplies(line: ParityLine): string {
  switch (line.by) {
    case 'cli': return "The agent's own program";
    case 'runner': return 'This computer, from outside the agent';
    case 'delegated': return `${line.to} itself`;
    case 'missing': {
      const reason = line.reason.replace(/^[A-Z]+\d+:\s*/, '');
      return reason.startsWith(BY_DESIGN)
        ? `Nobody, and it cannot be: ${reason.slice(BY_DESIGN.length).trim()}`
        : `Nobody yet: ${reason}`;
    }
  }
}

/**
 * For each containment rule, who applies it to the chosen agent. Where a fleet's runners report
 * different columns the first one that reports is shown: the rules belong to the agent, not to the machine.
 */
const ParityRules: Component<{ reports: Partial<HarnessCapability>[] }> = (props) => {
  const lines = () => props.reports.find((report) => report.parity?.length)?.parity ?? null;
  return (
    <Show when={lines()}>
      {(rows) => (
        <details data-testid="parity-rules" class="mt-1 text-xs">
          <summary class="cursor-pointer" style={{ color: 'var(--color-text-secondary)' }}>Who applies each rule</summary>
          <ul class="mt-1 space-y-0.5">
            <For each={rows()}>
              {(line) => (
                <li data-testid={`parity-${line.rule}`} data-by={line.by}>
                  <span>{line.label}: </span>
                  <span style={{ color: 'var(--color-text-secondary)' }}>{whoApplies(line)}</span>
                </li>
              )}
            </For>
          </ul>
        </details>
      )}
    </Show>
  );
};

export default ParityRules;
