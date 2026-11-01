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

const CLAUDE = {
  harness_kind: 'claude-code', installed_version: '2.0', probe_error: null, probed_at: '',
  decisions: { support: 'supported', reason: 'it asks before a tool call' },
  features: {
    cancel: { support: 'supported', reason: null },
    resume: { support: 'unsupported', reason: 'a run cannot be taken back after a restart' },
    usage: { support: 'advisory', reason: 'read from the result line' },
  },
};

const cell = (c: HTMLElement, kind: string, id: string) =>
  c.querySelector<HTMLElement>(`[data-testid="cap-${id}"][data-agent="${kind}"]`)!;
const mark = (el: HTMLElement) => el.querySelector('[aria-hidden="true"]')!.textContent;

describe('AgentCapabilities', () => {
  it('is a real table: agents are the column headers and the five capabilities the row headers', () => {
    const c = mount([runner([CLAUDE])]);
    const table = c.querySelector('table')!;
    expect(table.querySelector('caption')!.textContent).toContain('What each agent can do');
    const columns = Array.from(table.querySelectorAll('thead th[scope="col"]')).map((th) => th.getAttribute('data-testid'));
    expect(columns).toEqual(['agent-claude-code', 'agent-codex', 'agent-opencode', 'agent-docket']);
    const rows = Array.from(table.querySelectorAll('tbody th[scope="row"]')).map((th) => th.textContent);
    expect(rows).toEqual(['Stop a run', 'Pick a run back up after a restart', 'Answer questions', 'Propose items', 'Report usage']);
    expect(table.querySelectorAll('tbody td')).toHaveLength(20);
  });

  it('marks each cell as can, cannot or not reported, with the agent\'s note behind "How we know"', () => {
    const c = mount([runner([CLAUDE])]);
    const claude = c.querySelector<HTMLElement>('[data-testid="agent-claude-code"]')!;
    expect(claude.textContent).toContain('Claude Code');
    expect(claude.querySelector('[data-testid="agent-can"]')!.textContent).toBe('Can stop a run, answer questions and report usage.');

    const cancel = cell(c, 'claude-code', 'cancel');
    expect(mark(cancel)).toBe('✓');
    expect(cancel.getAttribute('aria-disabled')).toBeNull();
    expect(cancel.getAttribute('data-state')).toBe('can');

    const resume = cell(c, 'claude-code', 'resume');
    expect(mark(resume)).toBe('✗');
    expect(resume.getAttribute('aria-disabled')).toBe('true');
    const why = resume.querySelector('details')!;
    expect(why.querySelector('summary')!.textContent).toContain('How we know');
    expect(why.querySelector('p')!.textContent).toBe('Cannot pick a run back up after a restart · a run cannot be taken back after a restart');

    const usage = cell(c, 'claude-code', 'usage');
    expect(usage.querySelector('details p')!.textContent).toBe('Can report usage · read from the result line');
    expect(cell(c, 'claude-code', 'decisions').querySelector('details p')!.textContent).toContain('it asks before a tool call');

    const proposals = cell(c, 'claude-code', 'proposals');
    expect(mark(proposals)).toBe('—');
    expect(proposals.getAttribute('aria-disabled')).toBe('true');
    expect(proposals.getAttribute('data-state')).toBe('not_reported');
    expect(proposals.querySelector('details p')!.textContent).toBe('Not reported by Claude Code');
  });

  it('gives a screen reader the whole line of a cell, not just its mark', () => {
    const c = mount([runner([CLAUDE])]);
    expect(cell(c, 'claude-code', 'cancel').querySelector('.sr-only')!.textContent).toBe('Can stop a run. ');
    expect(cell(c, 'claude-code', 'resume').querySelector('.sr-only')!.textContent).toBe('Cannot pick a run back up after a restart. ');
  });

  it('lists an agent no computer reports as a column of dashes with the reason in its header, and says that each task can use a different agent', () => {
    const c = mount([]);
    for (const kind of ['claude-code', 'codex', 'opencode', 'docket']) {
      expect(c.querySelector(`[data-testid="agent-${kind}"]`), kind).not.toBeNull();
    }
    const codex = c.querySelector('[data-testid="agent-codex"]')!;
    expect(codex.textContent).toContain('No computer reports Codex');
    expect(codex.querySelector('[data-reported]')).toBeNull();
    for (const id of ['cancel', 'resume', 'decisions', 'proposals', 'usage']) {
      const dash = cell(c, 'codex', id);
      expect(mark(dash)).toBe('—');
      expect(dash.querySelector('details')).toBeNull();
      expect(dash.querySelector('.sr-only')!.textContent).toBe('Not reported by Codex');
    }
    expect(c.textContent).toContain('Each task can use a different agent');
    expect(c.textContent).not.toMatch(/harness|pod|blueprint|archetype/i);
  });
});
