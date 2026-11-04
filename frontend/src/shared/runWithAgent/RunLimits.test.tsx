import { describe, expect, it, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import RunLimits from './RunLimits';
import type { RunLimit } from '../execution/api';
import { flush } from '../../test/flush';

const disposers: Array<() => void> = [];

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

const limit = (agent: 'docket' | 'claude-code'): RunLimit => ({
  tokens: 1_000_000,
  source: 'profile',
  enforced_by: agent,
  sentence: '1 000 000 tokens · from the profile Implementer',
  used: 1000,
});

function mount(props: Parameters<typeof RunLimits>[0]) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes('/agent-profiles')) {
      return new Response(
        JSON.stringify({ protocol_version: 1, data: [{ agent_profile_id: 'prof-1', name: 'Implementer', tool_policy: { agent: 'docket', docket: { team: 'saas-dev' } } }] }),
        { status: 200 },
      );
    }
    if (url.endsWith('/teams')) {
      return new Response(
        JSON.stringify({
          teams: [
            { name: 'saas-dev', harness: 'docket', reported: true, profiles: [], report: { name: 'saas-dev', violations: [], paused: 'budget', budget: { tokens: 2_000_000, used: 2_000_000, window: 'day' } } },
          ],
        }),
        { status: 200 },
      );
    }
    return new Response('{}', { status: 200 });
  }) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <RunLimits {...props} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

const ids = (c: HTMLElement) => [...c.querySelectorAll('[data-testid^="limit-"]:not([data-testid$="-owner"])')].map((e) => e.getAttribute('data-testid'));

describe('RunLimits', () => {
  it('draws the run, step and team rows of a docket run, each with its owner', async () => {
    const c = mount({
      limit: limit('docket'),
      terminalReason: { code: 'step_budget', steps: [{ role: 'implementer', budget: { bound: 'toolCalls', limit: 40, used: 40, source: 'default', treeChanged: true, raise: 'AGENT_LOOP_MAX_TOOL_CALLS' } }] },
      profileId: 'prof-1',
      agentKind: 'docket',
    });
    await flush();
    await flush();
    expect(ids(c)).toEqual(['limit-run', 'limit-step', 'limit-team']);
    expect(c.querySelector('[data-testid="limit-run-owner"]')!.textContent).toBe('Applied by docket');
    expect(c.querySelector('[data-testid="limit-step-owner"]')!.textContent).toBe('Applied by docket on its machine');
    expect(c.querySelector('[data-testid="limit-team"]')!.textContent).toContain('2 000 000 tokens each day');
    expect(c.querySelector('[data-testid="limit-team"]')!.textContent).toContain('Paused: the budget is spent');
    expect(c.querySelector('[data-testid="limit-step"]')!.textContent).toContain('Raise it with AGENT_LOOP_MAX_TOOL_CALLS');
  });

  it("shows one row for a run on an agent with no team, applied by Tack's runner", async () => {
    const c = mount({ limit: limit('claude-code'), terminalReason: { result: 'done' }, profileId: null, agentKind: 'claude-code' });
    await flush();
    expect(ids(c)).toEqual(['limit-run']);
    expect(c.querySelector('[data-testid="limit-run-owner"]')!.textContent).toBe("Applied by Tack's runner");
  });

  it('draws nothing for a run whose server sent no limit and whose agent reported no more', async () => {
    const c = mount({ limit: null, terminalReason: null, profileId: null });
    await flush();
    expect(c.querySelector('[data-testid="run-limits"]')).toBeNull();
  });
});
