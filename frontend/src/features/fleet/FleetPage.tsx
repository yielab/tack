import { type Component, createResource, For, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Badge, Button, EmptyState, Skeleton } from '../../shared/ui';
import { fleetApi, isOrchDisabled, type FleetRow as FleetRowData } from './api';
import FleetRow from './FleetRow';
import RunnerFleetSection from './runnerFleet/RunnerFleetSection';

const COLUMNS = [
  'Project',
  'Pod health',
  'Roster',
  'Last activity',
  'Burn vs budget',
  'Gateway',
  'Approvals',
];

const thStyle = {
  padding: '10px 14px',
  'text-align': 'left' as const,
  'font-size': '10.5px',
  'font-weight': 700,
  'letter-spacing': '.05em',
  'text-transform': 'uppercase' as const,
  color: 'var(--color-text-tertiary)',
  'border-bottom': '1px solid var(--color-border-light)',
  'white-space': 'nowrap' as const,
};

/** Loading placeholder — three skeleton rows shaped like the real table. */
const LoadingRows: Component = () => (
  <>
    <For each={[0, 1, 2]}>
      {() => (
        <tr>
          <For each={COLUMNS}>
            {() => (
              <td style={{ padding: '14px' }}>
                <Skeleton height="14px" />
              </td>
            )}
          </For>
        </tr>
      )}
    </For>
  </>
);

/** Shown when the request succeeds but no plane has ever been registered, or
 *  when orchestration is enabled yet no project has an `orch_link` — the
 *  state most people see first, since no existing install has this set up. */
const RegisterPlaneEmptyState: Component = () => (
  <EmptyState
    icon="🛰️"
    title="No control planes registered"
    description={
      'Register an agent-fleet control plane (e.g. a running docket instance) to see its pods, roster, ' +
      'and burn here. Enable orchestration with TACK_ORCH_ENABLE=true, then register a plane via ' +
      'POST /api/control-planes with its base URL and (optional) bearer token — see the Orchestration guide.'
    }
  />
);

/** Shown when orchestration is off — the default state for every existing
 *  install, since orchestration is off unless an operator opts in (TODO.md
 *  §0 rule 8). Previously this told the operator to set `TACK_ORCH_ENABLE`
 *  and restart the server by hand — a dead end with no in-app next step.
 *  Card E2 (Phase 39) makes the flag UI-toggleable, so this now links
 *  straight to the guided setup instead. */
const OrchDisabledEmptyState: Component = () => {
  const navigate = useNavigate();
  return (
    <EmptyState
      icon="🔌"
      title="Agent-fleet orchestration is disabled"
      description="Turn it on to see agent pods, health, and estimated burn here — Tack will poll a control plane you register and can dispatch work to it."
      action={
        <Button onClick={() => navigate('/settings?section=orchestration')}>
          Set up orchestration
        </Button>
      }
    />
  );
};

const ErrorState: Component<{ onRetry: () => void }> = (props) => (
  <EmptyState
    icon="⚠️"
    title="Couldn't load fleet status"
    description="The request to the server failed. Check your connection and try again."
    action={<Button onClick={props.onRetry}>Retry</Button>}
  />
);

/**
 * Fleet page. Two structurally and visually distinct systems live here —
 * never conflated, per III.0's vocabulary rule that `Runner`/`Fleet` (this
 * cycle) and Docket's control-plane roster are different domain concepts
 * that happen to share the word "fleet":
 *
 * 1. **Runner Fleet** (`runnerFleet/RunnerFleetSection.tsx`, Part III,
 *    TODO.md III-E3) — the primary content: enroll/revoke runners, and
 *    manage fleets/agent profiles/model profiles for the harness-agnostic
 *    execution runner. New in this card.
 * 2. **Legacy: Docket control planes** (below, unchanged from card A5) —
 *    one row per Tack project linked to a Part II agent-fleet control
 *    plane (docket or compatible). Read-only; dispatch/approval actions
 *    live on the Approvals/Economics pages. Every dollar figure here is an
 *    estimate, never billed spend (TODO.md §0 rule 6) — see
 *    `format.ts#formatEstimatedCost`. Stale planes (`unreachable`/
 *    `unknown`) never render a confident-looking `0`/`$0.00` — see
 *    `FleetRow.tsx`'s `stale()` branch.
 *
 * Nothing about section 2's own components, tests, or behavior changed in
 * this card — only its position on the page and the heading above it.
 */
const FleetPage: Component = () => {
  const [fleet, { refetch }] = createResource(() => fleetApi.list());

  const rows = (): FleetRowData[] => fleet()?.rows ?? [];
  const disabled = () => isOrchDisabled(fleet.error);
  const failed = () => fleet.error !== undefined && !disabled();

  return (
    <div>
      <div class="mb-6">
        <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
          Fleet
        </h1>
        <p class="mt-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Enroll runners and manage fleets and agent/model profiles for Tack's harness-agnostic execution
          runner.
        </p>
      </div>

      <RunnerFleetSection />

      <div class="mt-10 mb-6 border-t pt-8" style={{ 'border-color': 'var(--color-border-light)' }}>
        <div class="flex items-center gap-2">
          <h2 class="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Legacy: Docket control planes
          </h2>
          <Badge tone="neutral">Part II</Badge>
        </div>
        <p class="mt-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Agent pods working your projects via an earlier agent-fleet orchestration system — health,
          roster, and estimated burn per control plane. Unrelated to the runner fleet above; kept working
          unchanged.
        </p>
      </div>

      <Show when={fleet.loading}>
        <div class="overflow-x-auto rounded-lg" style={{ border: '1px solid var(--color-border-light)' }}>
          <table class="w-full" style={{ 'border-collapse': 'collapse' }}>
            <thead>
              <tr>
                <For each={COLUMNS}>{(col) => <th style={thStyle}>{col}</th>}</For>
              </tr>
            </thead>
            <tbody>
              <LoadingRows />
            </tbody>
          </table>
        </div>
      </Show>

      <Show when={!fleet.loading && disabled()}>
        <OrchDisabledEmptyState />
      </Show>

      <Show when={!fleet.loading && failed()}>
        <ErrorState onRetry={refetch} />
      </Show>

      <Show when={!fleet.loading && !disabled() && !failed() && rows().length === 0}>
        <RegisterPlaneEmptyState />
      </Show>

      <Show when={!fleet.loading && !disabled() && !failed() && rows().length > 0}>
        <div class="overflow-x-auto rounded-lg" style={{ border: '1px solid var(--color-border-light)' }}>
          <table class="w-full" style={{ 'border-collapse': 'collapse' }}>
            <caption class="sr-only">Agent fleet status by project</caption>
            <thead>
              <tr>
                <For each={COLUMNS}>{(col) => <th scope="col" style={thStyle}>{col}</th>}</For>
              </tr>
            </thead>
            <tbody>
              <For each={rows()}>{(row) => <FleetRow row={row} />}</For>
            </tbody>
          </table>
        </div>
      </Show>
    </div>
  );
};

export default FleetPage;
