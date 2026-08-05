import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AgentActivityTab from './AgentActivityTab';
import type { ItemAgentActivity, ItemAgentAttempt } from '../../../shared/agentActivity/api';

const disposers: Array<() => void> = [];

function mount(activity: ItemAgentActivity | null | undefined, loading = false) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <AgentActivityTab activity={activity} loading={loading} />, container);
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

function makeAttempt(p: Partial<ItemAgentAttempt> & { remote_task_id: string; attempt: number }): ItemAgentAttempt {
  return {
    remote_run_id: null,
    remote_status: 'running',
    dispatched_at: '2026-08-01T00:00:00Z',
    tokens_in: 1000,
    tokens_out: 500,
    cost_usd_estimated: null,
    pricing_snapshot_at: null,
    run: null,
    events: [],
    ...p,
  };
}

describe('AgentActivityTab — no activity', () => {
  it('shows an empty state, not an error, when there are no attempts or approvals', () => {
    const c = mount({ attempts: [], approvals: [] });
    expect(c.textContent).toContain('No agent activity yet');
  });

  it('tolerates a null/undefined activity payload without throwing', () => {
    expect(() => mount(null)).not.toThrow();
    expect(() => mount(undefined)).not.toThrow();
  });
});

describe('AgentActivityTab — loading', () => {
  it('shows a loading indicator and not the empty state while loading', () => {
    const c = mount(undefined, true);
    expect(c.textContent).toContain('Loading agent activity');
    expect(c.textContent).not.toContain('No agent activity yet');
  });
});

describe('AgentActivityTab — attempts', () => {
  const activity: ItemAgentActivity = {
    attempts: [
      makeAttempt({ remote_task_id: 't1', attempt: 1, remote_status: 'failed', tokens_in: 2000, tokens_out: 800 }),
      makeAttempt({ remote_task_id: 't2', attempt: 2, remote_status: 'running', tokens_in: 500, tokens_out: 100 }),
    ],
    approvals: [],
  };

  it('renders every attempt with tokens as the primary figure', () => {
    const c = mount(activity);
    expect(c.textContent).toContain('Attempt 1');
    expect(c.textContent).toContain('Attempt 2');
    expect(c.textContent).toContain('2.0k in / 800 out tokens');
  });

  it('groups by attempt, newest first, regardless of the order supplied', () => {
    const c = mount(activity);
    const headings = [...c.querySelectorAll('li')]
      .map((li) => li.textContent ?? '')
      .filter((t) => t.includes('Attempt'));
    // Attempt 2 (newer) must appear before Attempt 1 in DOM order.
    const idx2 = headings.findIndex((t) => t.includes('Attempt 2'));
    const idx1 = headings.findIndex((t) => t.includes('Attempt 1'));
    expect(idx2).toBeLessThan(idx1);
  });

  it('every money figure says "estimated" or is honestly reported unavailable — never a bare number', () => {
    const c = mount(activity);
    expect(c.textContent).not.toMatch(/\$0\.00(?! estimated)/);
    expect(c.textContent).toMatch(/estimated|unavailable/);
  });

  it('labels a real cost with "estimated" and states the pricing-snapshot date is unknown rather than omitting it', () => {
    const withCost: ItemAgentActivity = {
      attempts: [makeAttempt({ remote_task_id: 't1', attempt: 1, cost_usd_estimated: 4.5, pricing_snapshot_at: null })],
      approvals: [],
    };
    const c = mount(withCost);
    expect(c.textContent).toMatch(/\$4\.50 estimated/);
    expect(c.textContent).toMatch(/pricing snapshot date unknown/);
  });
});

describe('AgentActivityTab — pending approvals', () => {
  it('surfaces a pending approval prominently at the top', () => {
    const activity: ItemAgentActivity = {
      attempts: [makeAttempt({ remote_task_id: 't1', attempt: 1, remote_status: 'waiting_approval' })],
      approvals: [
        {
          token: 'apr-1',
          remote_task_id: 't1',
          agent: 'reviewer',
          action: 'merge branch',
          state: 'pending',
          requested_at: '2026-08-01T00:00:00Z',
          decided_at: null,
        },
      ],
    };
    const c = mount(activity);
    expect(c.textContent).toContain('1 approval pending');
    expect(c.textContent).toContain('reviewer');
    expect(c.textContent).toContain('merge branch');
  });

  it('does not show the pending banner once every approval is decided', () => {
    const activity: ItemAgentActivity = {
      attempts: [makeAttempt({ remote_task_id: 't1', attempt: 1, remote_status: 'done' })],
      approvals: [
        {
          token: 'apr-1',
          remote_task_id: 't1',
          agent: 'reviewer',
          action: 'merge branch',
          state: 'granted',
          requested_at: '2026-08-01T00:00:00Z',
          decided_at: '2026-08-01T00:05:00Z',
        },
      ],
    };
    const c = mount(activity);
    expect(c.textContent).not.toContain('approval pending');
  });
});

describe('AgentActivityTab — events_truncated', () => {
  it('shows no partial-history notice when events_truncated is false/absent', () => {
    const activity: ItemAgentActivity = {
      attempts: [makeAttempt({ remote_task_id: 't1', attempt: 1 })],
      approvals: [],
      events_truncated: false,
      events_retention_days: 90,
    };
    const c = mount(activity);
    expect(c.textContent).not.toContain('Partial history');
    expect(c.textContent).not.toMatch(/retention window/);
  });

  it('surfaces the retention window when events_truncated is true, without claiming a precise count', () => {
    const activity: ItemAgentActivity = {
      attempts: [makeAttempt({ remote_task_id: 't1', attempt: 1 })],
      approvals: [],
      events_truncated: true,
      events_retention_days: 90,
    };
    const c = mount(activity);
    expect(c.textContent).toContain('Partial history');
    expect(c.textContent).toMatch(/90-day/);
    expect(c.textContent).toMatch(/may have been aged out/);
    expect(c.textContent).not.toMatch(/\d+ events? (were|have been) (deleted|removed)/);
  });

  it('never shows the notice on the empty-activity state, even if the flag were somehow true', () => {
    const c = mount({ attempts: [], approvals: [], events_truncated: true, events_retention_days: 90 });
    expect(c.textContent).not.toContain('Partial history');
    expect(c.textContent).toContain('No agent activity yet');
  });
});

describe('AgentActivityTab — unrecognised remote_status', () => {
  it('renders the "failed" chip state (conservative degrade) without throwing', () => {
    const activity: ItemAgentActivity = {
      attempts: [makeAttempt({ remote_task_id: 't1', attempt: 1, remote_status: 'some_future_status' })],
      approvals: [],
    };
    expect(() => mount(activity)).not.toThrow();
    const c = mount(activity);
    expect(c.textContent).toContain('Failed');
  });
});
