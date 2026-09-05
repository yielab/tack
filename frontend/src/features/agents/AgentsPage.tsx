import { type Component } from 'solid-js';
import ExecutionToggle from './ExecutionToggle';
import ProviderKeyPanel from './ProviderKeyPanel';

/**
 * Minimal placeholder for "the Agents page" — VI-B3's own two panels
 * (turning the embedded runner on and handing it a provider key, ADR 0061
 * decisions 2 and 6), reachable so they have a real page to render on
 * before the fuller page composing every step of `§VI.0`'s surface map
 * lands. Not yet linked from the sidebar or the first-run banner — that
 * composition, plus moving `RunnerFleetSection` in from `FleetPage.tsx` and
 * everything else the surface map calls for, is the next Wave 16 card's
 * job (`docs/agent-handoffs/part-vi/README.md`'s VI-C1 block); this file is
 * what it extends, not a finished page in its own right.
 */
const AgentsPage: Component = () => {
  return (
    <div>
      <div class="mb-6">
        <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
          Agents
        </h1>
        <p class="mt-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Turn on agent execution on this machine and give it a model provider key.
        </p>
      </div>

      <ExecutionToggle />
      <ProviderKeyPanel />
    </div>
  );
};

export default AgentsPage;
