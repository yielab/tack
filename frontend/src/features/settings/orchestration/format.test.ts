import { describe, it, expect } from 'vitest';
import {
  BUDGET_PAUSE_NOTE,
  BUDGET_PROGRESS_CAVEAT,
  HEALTH_LABEL,
  HEALTH_TONE,
  POLICY_SCOPE_CAVEAT,
  budgetProgress,
  formatBudgetCap,
  formatDenialRate,
  formatPercent,
  relativeTime,
} from './format';

describe('formatBudgetCap', () => {
  it('reports "no budget cap set" for null', () => {
    expect(formatBudgetCap(null)).toBe('no budget cap set');
  });

  it('formats a real cap as currency, never as an estimate', () => {
    const out = formatBudgetCap(50);
    expect(out).toContain('$50.00');
    expect(out).toContain('cap');
    expect(out).not.toContain('estimated');
  });
});

describe('budgetProgress', () => {
  it('is null when cost is null (unlinked/unreachable — never a fabricated fraction)', () => {
    expect(budgetProgress(null, 50)).toBeNull();
  });

  it('is null when there is no budget cap set', () => {
    expect(budgetProgress(10, null)).toBeNull();
  });

  it('is null when the cap is zero or negative', () => {
    expect(budgetProgress(10, 0)).toBeNull();
    expect(budgetProgress(10, -5)).toBeNull();
  });

  it('is "success" tone comfortably under the cap', () => {
    const p = budgetProgress(10, 100);
    expect(p).not.toBeNull();
    expect(p!.fraction).toBeCloseTo(0.1);
    expect(p!.tone).toBe('success');
  });

  it('is "warning" tone at or above 70% of the cap', () => {
    const p = budgetProgress(75, 100);
    expect(p!.tone).toBe('warning');
  });

  it('is "danger" tone once spend reaches or exceeds the cap, and is not clamped at 1', () => {
    const atCap = budgetProgress(100, 100);
    expect(atCap!.tone).toBe('danger');
    expect(atCap!.fraction).toBeCloseTo(1);

    const overCap = budgetProgress(150, 100);
    expect(overCap!.tone).toBe('danger');
    expect(overCap!.fraction).toBeCloseTo(1.5);
  });
});

describe('formatPercent', () => {
  it('formats a fraction as a rounded percentage', () => {
    expect(formatPercent(0.2)).toBe('20%');
    expect(formatPercent(1.5)).toBe('150%');
  });
});

describe('formatDenialRate', () => {
  it('reports "no tool-call data observed yet" for null — never a fabricated 0%', () => {
    expect(formatDenialRate(null)).toBe('no tool-call data observed yet');
  });

  it('formats a real rate as a percentage of tool calls denied', () => {
    expect(formatDenialRate(0.2)).toBe('20% of tool calls denied');
  });
});

describe('relativeTime', () => {
  it('reports "never" for null', () => {
    expect(relativeTime(null)).toBe('never');
  });

  it('reports "unknown" for an unparseable date', () => {
    expect(relativeTime('not-a-date')).toBe('unknown');
  });

  it('reports a relative duration for a recent timestamp', () => {
    const fiveMinAgo = new Date(Date.now() - 5 * 60_000).toISOString();
    expect(relativeTime(fiveMinAgo)).toBe('5m ago');
  });
});

describe('health label/tone maps', () => {
  it('cover all four health states distinctly', () => {
    const states = ['healthy', 'degraded', 'unreachable', 'unknown'] as const;
    const labels = new Set(states.map((s) => HEALTH_LABEL[s]));
    const tones = states.map((s) => HEALTH_TONE[s]);
    expect(labels.size).toBe(4);
    expect(new Set(tones).size).toBeGreaterThan(1);
  });
});

describe('mandatory caveat copy', () => {
  it('BUDGET_PROGRESS_CAVEAT names the compounding-estimate problem explicitly', () => {
    expect(BUDGET_PROGRESS_CAVEAT.toLowerCase()).toContain('estimate of a fraction of an estimate');
  });

  it('BUDGET_PAUSE_NOTE names the real CLI remedy, not a fabricated Tack control', () => {
    expect(BUDGET_PAUSE_NOTE).toContain('docket profile');
    expect(BUDGET_PAUSE_NOTE).toContain('--resume');
  });

  it('POLICY_SCOPE_CAVEAT says the numbers are control-plane-wide', () => {
    expect(POLICY_SCOPE_CAVEAT.toLowerCase()).toContain('control plane');
  });
});
