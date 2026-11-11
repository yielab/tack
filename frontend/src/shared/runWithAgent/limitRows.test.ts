import { describe, expect, it } from 'vitest';
import type { RunLimit } from '../execution/api';
import { limitRows, limitsSummary } from './limitRows';

const docketRun: RunLimit = {
  tokens: 1_600_000,
  source: 'profile',
  enforced_by: 'docket',
  sentence: '1 600 000 tokens · from the profile Implementer',
  used: 420_000,
};

const stoppedAtRounds = {
  code: 'step_budget',
  steps: [
    { role: 'lead', ok: true },
    { role: 'implementer', ok: false, budget: { bound: 'rounds', limit: 30, used: 30, source: 'default', treeChanged: true, raise: 'AGENT_LOOP_MAX_ITERATIONS' } },
  ],
};

describe('limitRows', () => {
  it('draws the three levels of a docket run, each with who applies it', () => {
    const rows = limitRows({
      limit: docketRun,
      terminalReason: stoppedAtRounds,
      team: { name: 'saas-dev', paused: 'budget', budget: { tokens: 2_000_000, used: 2_000_000, window: 'day', window_start: null } },
    });
    expect(rows.map((r) => r.id)).toEqual(['run', 'step', 'team']);
    expect(rows[0]).toMatchObject({ owner: 'Applied by docket' });
    expect(rows[0].lines).toEqual(['1 600 000 tokens · from the profile Implementer', '420 000 used so far']);
    expect(rows[1].owner).toBe('Applied by docket on its machine');
    expect(rows[1].lines).toEqual(["Rounds: 30 of 30", "Limit docket's default", 'Raise it with AGENT_LOOP_MAX_ITERATIONS']);
    expect(rows[2].owner).toBe('Applied by docket');
    expect(rows[2].lines).toEqual(['2 000 000 tokens each day', '2 000 000 used', 'Paused: the budget is spent']);
    expect(limitsSummary(rows)).toBe('Run · Step · Team');
  });

  it("shows a claude-code run's one row, applied by Tack's runner", () => {
    const rows = limitRows({
      limit: { tokens: 2_000_000, source: 'provisional', enforced_by: 'claude-code', sentence: '2 000 000 tokens · provisional', used: null },
      terminalReason: { result: 'done' },
      team: null,
    });
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ id: 'run', owner: "Applied by Tack's runner", lines: ['2 000 000 tokens · provisional'] });
  });

  it('leaves out a step row for a tokens stop and a team row for a team that reported no budget', () => {
    const rows = limitRows({
      limit: docketRun,
      terminalReason: { code: 'budget', steps: [{ role: 'implementer', budget: { bound: 'tokens', limit: 5, used: 5, source: 'task', treeChanged: false, raise: null } }] },
      team: { name: 'saas-dev', budget: null },
    });
    expect(rows.map((r) => r.id)).toEqual(['run']);
  });

  it('says a team with no limit has none', () => {
    const rows = limitRows({ team: { name: 'saas-dev', budget: { tokens: null, used: 10, window: 'none' } } });
    expect(rows).toHaveLength(1);
    expect(rows[0].lines).toEqual(['No token limit', '10 used']);
  });
});
