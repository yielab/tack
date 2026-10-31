import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AgentCapabilities from './AgentCapabilities';
import type { RunnerSummary } from '../../shared/execution/api';

afterEach(() => {
  document.body.innerHTML = '';
});

const runner = (harnesses: unknown[]): RunnerSummary =>
  ({
    runner_id: 'r1', name: 'This computer', state: 'active', labels: null, labels_raw: '{}', total_capacity: 1, available_capacity: 1,
    capability_snapshot: { harnesses }, capability_snapshot_raw: '', protocol_version: 1, runner_version: '1', last_heartbeat_at: null,
    online: true, revoked_at: null, fleet_ids: [], created_at: '', updated_at: '',
  }) as RunnerSummary;

const mount = (runners: RunnerSummary[]) => {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <AgentCapabilities runners={runners} />, container);
  return container;
};

describe('AgentCapabilities', () => {
  it('names each agent, says what it can do from what it reported, and shows the rest disabled with a reason', () => {
    const c = mount([
      runner([
        {
          harness_kind: 'claude-code', installed_version: '2.0', probe_error: null, probed_at: '',
          decisions: { support: 'supported', reason: 'it asks before a tool call' },
          features: {
            cancel: { support: 'supported', reason: null },
            resume: { support: 'unsupported', reason: 'a run cannot be taken back after a restart' },
            usage: { support: 'advisory', reason: 'read from the result line' },
          },
        },
      ]),
    ]);
    const claude = c.querySelector<HTMLElement>('[data-testid="agent-claude-code"]')!;
    expect(claude.textContent).toContain('Claude Code');
    expect(claude.querySelector('[data-testid="agent-can"]')!.textContent).toBe('Can stop a run, answer questions and report usage.');
    const resume = claude.querySelector<HTMLElement>('[data-testid="cap-resume"]')!;
    expect(resume.getAttribute('aria-disabled')).toBe('true');
    expect(resume.textContent).toContain('Cannot pick a run back up after a restart');
    expect(resume.textContent).toContain('a run cannot be taken back after a restart');
    const proposals = claude.querySelector<HTMLElement>('[data-testid="cap-proposals"]')!;
    expect(proposals.getAttribute('aria-disabled')).toBe('true');
    expect(proposals.textContent).toBe('Not reported by Claude Code');
    expect(claude.querySelector('[data-testid="cap-cancel"]')!.getAttribute('aria-disabled')).toBeNull();
  });

  it('lists an agent no computer reports, and says that each task can use a different agent', () => {
    const c = mount([]);
    for (const kind of ['claude-code', 'codex', 'opencode', 'docket']) {
      expect(c.querySelector(`[data-testid="agent-${kind}"]`), kind).not.toBeNull();
    }
    expect(c.querySelector('[data-testid="agent-codex"]')!.textContent).toContain('No computer reports Codex');
    expect(c.textContent).toContain('Each task can use a different agent');
    expect(c.textContent).not.toMatch(/harness|pod|blueprint|archetype/i);
  });
});
