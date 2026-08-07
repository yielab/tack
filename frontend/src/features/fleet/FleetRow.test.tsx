import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import FleetRow from './FleetRow';
import type { FleetRow as FleetRowData } from './api';
import type { Capabilities } from '../../shared/orch/capabilities';

const disposers: Array<() => void> = [];

/** FleetRow renders a <tr>, which is only valid inside a <table>; mount it
 *  there so the DOM shape matches production and nothing about the assertions
 *  depends on browser table-repair behavior. Wrapped in a router because it
 *  renders an <A> link to the project. */
function mount(row: FleetRowData) {
  const container = document.createElement('table');
  document.body.appendChild(container);
  const tbody = document.createElement('tbody');
  container.appendChild(tbody);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={() => <FleetRow row={row} />} />
      </MemoryRouter>
    ),
    tbody,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

/** A realistic docket capabilities payload, matching
 *  `crates/tack-orch/src/adapters/docket.rs::capabilities()`'s real reasons —
 *  a drifted fixture here would silently defeat the "reason came from the
 *  payload" assertions below. */
const docketCapabilities: Capabilities = {
  dispatch: true,
  cancel: false,
  pause: {
    level: 'unsupported',
    reason:
      'docket exposes no pause endpoint over HTTP in either direction; from the docket ' +
      'CLI, run `docket profile <pod-id> --resume` to clear a budget-triggered pause',
  },
  resume: { level: 'unsupported', reason: 'docket exposes no resume endpoint over HTTP' },
  event_scope: { level: 'project', reason: "docket's trace stream is scoped per project" },
  artifacts: false,
  decisions: { level: 'poll', reason: 'read via GET /approvals on the poll cadence' },
  usage: { level: 'from_provider', reason: 'docket estimates cost/token usage itself' },
  model_selection: { level: 'unsupported', reason: 'docket owns its own model routing' },
  runtimes: true,
  plane_metrics: true,
  provisioning: true,
};

const baseRow: FleetRowData = {
  project_id: 'proj-1',
  project_name: 'Adapta',
  control_plane_id: 'cp-1',
  control_plane_name: 'home-docket',
  control_plane_kind: 'docket',
  health: 'healthy',
  last_seen_at: new Date().toISOString(),
  consecutive_failures: 0,
  capabilities: docketCapabilities,
  gateway: 'active',
  roster: [
    { id: 'a1', name: 'Backend Dev', role: 'backend', model: 'claude-sonnet-5' },
    { id: 'a2', name: 'Reviewer', role: 'reviewer', model: 'claude-opus-5' },
  ],
  last_activity_at: new Date(Date.now() - 3 * 60 * 1000).toISOString(),
  tokens_in: 128_000,
  tokens_out: 42_000,
  cost_usd_estimated: 4.32,
  pricing_snapshot_at: '2026-07-01T00:00:00Z',
  budget_usd: 50,
  pending_approval_count: 0,
};

describe('FleetRow — healthy state', () => {
  it('renders the project, roster, and a live burn figure', () => {
    const c = mount(baseRow);
    expect(c.textContent).toContain('Adapta');
    expect(c.textContent).toContain('backend · claude-sonnet-5');
    expect(c.textContent).toContain('reviewer · claude-opus-5');
    expect(c.textContent).toContain('Healthy');
  });

  it('shows tokens at least as prominently as the dollar estimate', () => {
    const c = mount(baseRow);
    expect(c.textContent).toContain('128.0k in / 42.0k out tokens');
  });

  it('labels the cost figure "estimated" with its pricing snapshot date', () => {
    const c = mount(baseRow);
    expect(c.textContent).toMatch(/\$4\.32 estimated/);
    expect(c.textContent).toMatch(/pricing as of/);
  });

  it('does not show a "Last seen" caption when healthy', () => {
    const c = mount(baseRow);
    expect(c.textContent).not.toContain('Last seen');
  });

  it('renders a plain (non-muted) row background', () => {
    const c = mount(baseRow);
    const tr = c.querySelector('tr')!;
    expect(tr.style.background).toBe('transparent');
  });
});

describe('FleetRow — degraded state', () => {
  const degradedRow: FleetRowData = { ...baseRow, health: 'degraded', consecutive_failures: 3 };

  it('shows a distinct amber chip and a "Last seen" caption, but keeps figures live', () => {
    const c = mount(degradedRow);
    expect(c.textContent).toContain('Degraded');
    expect(c.textContent).toContain('Last seen');
    // Degraded data is still recent enough to show — not blanked like unreachable.
    expect(c.textContent).toContain('128.0k in / 42.0k out tokens');
  });

  it('keeps the plain row background — visually distinct from the muted unreachable row', () => {
    const c = mount(degradedRow);
    const tr = c.querySelector('tr')!;
    expect(tr.style.background).toBe('transparent');
  });
});

describe('FleetRow — unreachable state (never a confident-looking zero)', () => {
  const unreachableRow: FleetRowData = {
    ...baseRow,
    health: 'unreachable',
    consecutive_failures: 12,
    last_seen_at: new Date(Date.now() - 4 * 60 * 1000).toISOString(),
    tokens_in: 0,
    tokens_out: 0,
    cost_usd_estimated: null,
    pending_approval_count: 0,
    gateway: 'active', // stale cached reading — must NOT be shown as current
  };

  it('applies a muted background token (never plain opacity — see FleetRow.tsx doc comment)', () => {
    const c = mount(unreachableRow);
    const tr = c.querySelector('tr')!;
    expect(tr.style.background).toBe('var(--color-bg-subtle)');
    expect(tr.style.opacity).toBe('');
  });

  it('reads "last seen 4m ago" rather than a blank or a live-looking figure', () => {
    const c = mount(unreachableRow);
    expect(c.textContent).toMatch(/Last seen 4m ago/);
  });

  it('never renders a confident-looking $0.00 for the burn figure', () => {
    const c = mount(unreachableRow);
    expect(c.textContent).not.toMatch(/\$0\.00/);
    expect(c.textContent).toContain('no fresh estimate');
  });

  it('never renders a bare "0 tokens" line for an unreachable plane', () => {
    const c = mount(unreachableRow);
    expect(c.textContent).not.toMatch(/0.*in.*\/.*0.*out tokens/);
  });

  it('does not present the stale cached gateway reading as current — shows Unknown instead', () => {
    const c = mount(unreachableRow);
    expect(c.textContent).toContain('Unknown');
    expect(c.textContent).not.toContain('Active');
  });

  it('does not present pending-approval count as a trustworthy 0', () => {
    const c = mount(unreachableRow);
    // The dash placeholder appears; a bare standalone "0" pending-approvals
    // reading must not be present for this row.
    const approvalsCell = c.querySelectorAll('td')[6];
    expect(approvalsCell.textContent?.trim()).toBe('—');
  });
});

describe('FleetRow — capability negotiation (card G1)', () => {
  it("renders docket's real pause reason from the capability payload, not a hard-coded string", () => {
    const c = mount(baseRow);
    expect(c.textContent).toContain('Pause:');
    expect(c.textContent).toContain('docket profile <pod-id> --resume');
  });

  it('renders a DIFFERENT reason verbatim for a different payload — proving the text traces to the capability, not to control_plane_kind', () => {
    const rowWithDifferentReason: FleetRowData = {
      ...baseRow,
      capabilities: {
        ...docketCapabilities,
        pause: { level: 'unsupported', reason: 'a hypothetical adapter-specific reason' },
      },
    };
    const c = mount(rowWithDifferentReason);
    expect(c.textContent).toContain('a hypothetical adapter-specific reason');
    expect(c.textContent).not.toContain('docket profile');
  });

  it('renders no Pause note once the capability reports Supported', () => {
    const rowWithSupportedPause: FleetRowData = {
      ...baseRow,
      capabilities: { ...docketCapabilities, pause: { level: 'supported', reason: 'moot' } },
    };
    const c = mount(rowWithSupportedPause);
    expect(c.textContent).not.toContain('Pause:');
  });

  it('renders no Pause note at all when capabilities is null (the unconfigured case)', () => {
    const rowWithNoCapabilities: FleetRowData = { ...baseRow, capabilities: null };
    const c = mount(rowWithNoCapabilities);
    expect(c.textContent).not.toContain('Pause:');
  });
});

describe('FleetRow — unconfigured state (credentials missing, never polled)', () => {
  const unconfiguredRow: FleetRowData = {
    ...baseRow,
    health: 'unconfigured',
    last_seen_at: null,
    consecutive_failures: 0,
    tokens_in: 0,
    tokens_out: 0,
    cost_usd_estimated: null,
    roster: [],
    capabilities: null,
  };

  it('reads a credentials-missing caption rather than "Not yet connected" or a fabricated "last seen"', () => {
    const c = mount(unconfiguredRow);
    expect(c.textContent).toContain('Credentials missing');
    expect(c.textContent).not.toContain('Not yet connected');
    expect(c.textContent).not.toContain('Last seen');
  });

  it('applies the same muted background as unreachable/unknown — no trustworthy data to show', () => {
    const c = mount(unconfiguredRow);
    const tr = c.querySelector('tr')!;
    expect(tr.style.background).toBe('var(--color-bg-subtle)');
  });
});

describe('FleetRow — unknown state (never polled)', () => {
  const unknownRow: FleetRowData = {
    ...baseRow,
    health: 'unknown',
    last_seen_at: null,
    consecutive_failures: 0,
    tokens_in: 0,
    tokens_out: 0,
    cost_usd_estimated: null,
    roster: [],
  };

  it('reads "Not yet connected" rather than a fabricated "last seen"', () => {
    const c = mount(unknownRow);
    expect(c.textContent).toContain('Not yet connected');
    expect(c.textContent).not.toContain('Last seen');
  });

  it('shows a roster-unavailable message rather than "0 agents"', () => {
    const c = mount(unknownRow);
    expect(c.textContent).toContain('No cached roster — plane unreachable');
    expect(c.textContent).not.toMatch(/0 agents/);
  });
});
