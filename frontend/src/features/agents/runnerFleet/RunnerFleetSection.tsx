import { type Component, Show, createSignal } from 'solid-js';
import { Tabs } from '../../../shared/ui';
import EnrollmentPanel from './EnrollmentPanel';
import FleetsPanel from './FleetsPanel';
import AgentProfilesPanel from './AgentProfilesPanel';

const TABS = [
  { id: 'runners', label: 'Runners' },
  { id: 'fleets', label: 'Fleets' },
  { id: 'agent-profiles', label: 'Agent profiles' },
] as const;

type TabId = (typeof TABS)[number]['id'];

/**
 * Runner-fleet management surface: enrollment/revocation, fleets, agent
 * profiles — everything `frontend/src/shared/execution`'s
 * `runnersApi`/`fleetsApi`/`agentProfilesApi` expose a
 * real, wired endpoint for. `GET /runners` exists and is used elsewhere
 * (`AgentsPage.tsx`, `RunWithAgentModal.tsx`), but the `EnrollmentPanel`
 * tab mounted below does not call it — see that file's header comment.
 *
 * Mounted under the Agents page's Advanced section (`AdvancedSection.tsx`).
 * This is the runner-v1 execution fleet — a named group of runners sharing
 * a concurrency limit and default policy — a distinct concept from the
 * legacy Docket control plane's per-project pod roster, which happened to
 * share the word "fleet".
 */
const RunnerFleetSection: Component = () => {
  const [active, setActive] = createSignal<TabId>('runners');

  return (
    <Tabs tabs={[...TABS]} active={active()} onChange={(id) => setActive(id as TabId)}>
      <Show when={active() === 'runners'}>
        <EnrollmentPanel />
      </Show>
      <Show when={active() === 'fleets'}>
        <FleetsPanel />
      </Show>
      <Show when={active() === 'agent-profiles'}>
        <AgentProfilesPanel />
      </Show>
    </Tabs>
  );
};

export default RunnerFleetSection;
