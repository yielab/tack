import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { createSignal } from 'solid-js';
import TeamStepsLine from './TeamSteps';
import { flush } from '../../test/flush';

const disposers: Array<() => void> = [];

type Ev = { kind: string; payload: unknown };
const started = (role: string): Ev => ({ kind: 'agent.hop_started', payload: { text: `${role} (model-x) started` } });
const finished = (role: string, cycle = 1): Ev => ({ kind: 'agent.hop_finished', payload: { text: `${role} (model-x) ok cycle ${cycle} gate 1` } });

const STEPS = [
  { name: 'write', kind: 'agent', role: 'lead', checked_by: 'reviewer', sends_back_to: 'write', sends_back_max: 2 },
  { name: 'review', kind: 'agent', role: 'reviewer', checked_by: 'command' },
  { name: 'tests', kind: 'command', checked_by: 'command' },
  { name: 'sign off', kind: 'ask', checked_by: 'person' },
  { name: 'deploy check', kind: 'wait', checked_by: 'external' },
];

const PROFILE = { agent_profile_id: 'ap_dev', name: 'Dev', instructions: '', tool_policy: { docket: { team: 'my-saas-dev' } }, limits: null };

function mount(opts: { steps?: unknown[] | null; events: () => Ev[]; agentKind?: string | null; profileId?: string | null; ended?: boolean }) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes('/agent-profiles')) return new Response(JSON.stringify({ protocol_version: 1, data: [PROFILE] }), { status: 200 });
    if (url.includes('/teams')) {
      const teams = opts.steps === undefined
        ? []
        : [{ name: 'my-saas-dev', harness: 'docket', profiles: [], reported: true, report: { name: 'my-saas-dev', profile: 'x', ...(opts.steps === null ? {} : { steps: opts.steps }) } }];
      return new Response(JSON.stringify({ teams }), { status: 200 });
    }
    return new Response('{}', { status: 200 });
  }) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <TeamStepsLine
        profileId={opts.profileId === undefined ? 'ap_dev' : opts.profileId}
        agentKind={opts.agentKind === undefined ? 'docket' : opts.agentKind}
        events={opts.events()}
        ended={opts.ended}
      />
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

const settle = async () => {
  for (let i = 0; i < 4; i++) await flush();
};
const step = (c: Element, name: string) => c.querySelector(`[data-step="${name}"]`)!;

describe('TeamSteps', () => {
  it('shows the steps the team reports in order, the running one marked, with what checks each in words', async () => {
    const c = mount({ steps: STEPS, events: () => [started('Lead')] });
    await settle();
    const items = [...c.querySelectorAll('[data-testid="team-steps"] > li')];
    expect(items.map((li) => li.getAttribute('data-step'))).toEqual(['write', 'review', 'tests', 'sign off', 'deploy check']);
    expect(step(c, 'write').getAttribute('data-state')).toBe('running');
    expect(step(c, 'write').textContent).toContain('Running');
    expect(step(c, 'write').textContent).toContain('a reviewer can send it back, up to 2 times');
    expect(step(c, 'review').getAttribute('data-state')).toBe('waiting');
    expect(step(c, 'review').textContent).toContain('checked by a command');
    expect(step(c, 'sign off').textContent).toContain('asks you');
    expect(step(c, 'deploy check').textContent).toContain('checked outside the team');
    expect(c.querySelector('[data-testid="team-steps-unreported"]')).toBeNull();
  });

  it('a step goes from waiting to done on its end event', async () => {
    const [events, setEvents] = createSignal<Ev[]>([]);
    const c = mount({ steps: STEPS, events });
    await settle();
    expect(step(c, 'write').getAttribute('data-state')).toBe('waiting');
    setEvents([started('Lead'), finished('Lead')]);
    await settle();
    expect(step(c, 'write').getAttribute('data-state')).toBe('done');
    expect(step(c, 'write').textContent).toContain('Done');
    expect(step(c, 'review').getAttribute('data-state')).toBe('waiting');
  });

  it('a step sent back returns to running, and the steps after it wait again', async () => {
    const c = mount({
      steps: STEPS,
      events: () => [started('Lead'), finished('Lead'), started('Reviewer'), finished('Reviewer', 1), started('Lead')],
    });
    await settle();
    expect(step(c, 'write').getAttribute('data-state')).toBe('running');
    expect(step(c, 'review').getAttribute('data-state')).toBe('waiting');
  });

  it('a step with no role carries no state mark', async () => {
    const c = mount({ steps: STEPS, events: () => [started('Lead'), finished('Lead'), started('Reviewer'), finished('Reviewer')] });
    await settle();
    for (const name of ['tests', 'sign off', 'deploy check']) {
      expect(step(c, name).hasAttribute('data-state')).toBe(false);
      expect(step(c, name).textContent).not.toMatch(/Done|Running|Waiting/);
    }
    expect(step(c, 'tests').textContent).toContain('runs a command');
    expect(step(c, 'deploy check').textContent).toContain('waits');
    expect(step(c, 'review').getAttribute('data-state')).toBe('done');
  });

  it('a team that reports no steps says its agent does not report them in advance', async () => {
    const c = mount({ steps: null, events: () => [started('Lead')] });
    await settle();
    expect(c.querySelector('[data-testid="team-steps"]')).toBeNull();
    expect(c.querySelector('[data-testid="team-steps-unreported"]')!.textContent).toBe('Docket does not report its steps in advance');
  });

  it('a run with no team and an unknown agent says nothing', async () => {
    const c = mount({ events: () => [], profileId: null, agentKind: null });
    await settle();
    expect(c.textContent).toBe('');
    const named = mount({ events: () => [], profileId: null, agentKind: 'codex' });
    await settle();
    expect(named.querySelector('[data-testid="team-steps-unreported"]')!.textContent).toBe('Codex does not report its steps in advance');
  });

  it('a run that ended with a step still running says it stopped there', async () => {
    const c = mount({ steps: STEPS, events: () => [started('Lead')], ended: true });
    await settle();
    expect(step(c, 'write').textContent).toContain('Stopped here');
    expect(step(c, 'write').textContent).not.toContain('Running');
  });
});
