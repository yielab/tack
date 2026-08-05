import { describe, it, expect } from 'vitest';
import {
  formatHours,
  formatRate,
  describeLeadTime,
  describeRework,
  describeCostPerItem,
} from './format';
import type { LeadTimeStat, ReworkStat } from './api';

describe('formatHours', () => {
  it('renders under-48h durations in hours', () => {
    expect(formatHours(2.5)).toBe('2.5h');
  });

  it('renders 48h+ durations in days', () => {
    expect(formatHours(72)).toBe('3.0d');
  });
});

describe('formatRate', () => {
  it('renders null as an explicit "n/a", never a bare 0%', () => {
    expect(formatRate(null)).toBe('n/a');
  });

  it('renders a fraction as a percentage', () => {
    expect(formatRate(0.25)).toBe('25.0%');
  });

  it('renders a genuine zero rate distinctly from unknown', () => {
    expect(formatRate(0)).toBe('0.0%');
    expect(formatRate(0)).not.toBe('n/a');
  });
});

function leadTime(overrides: Partial<LeadTimeStat> = {}): LeadTimeStat {
  return {
    sample_count: 0,
    below_min_sample: true,
    avg_hours: null,
    raw_hours: null,
    ...overrides,
  };
}

describe('describeLeadTime', () => {
  it('reports "no completed items yet" for a zero sample', () => {
    expect(describeLeadTime(leadTime())).toMatch(/no completed items yet/);
  });

  it('shows raw durations, not an average, below the minimum sample size', () => {
    const stat = leadTime({ sample_count: 2, below_min_sample: true, raw_hours: [1, 5] });
    const out = describeLeadTime(stat);
    expect(out).toMatch(/too few samples/);
    expect(out).toMatch(/1\.0h/);
    expect(out).toMatch(/5\.0h/);
    // The disclosure copy may explain why no average is shown, but must never
    // present one of the numbers itself as "the average".
    expect(out).not.toMatch(/\d+(\.\d+)?h average/);
  });

  it('shows an average once the minimum sample size is met', () => {
    const stat = leadTime({ sample_count: 10, below_min_sample: false, avg_hours: 4.5 });
    const out = describeLeadTime(stat);
    expect(out).toMatch(/average/);
    expect(out).toMatch(/4\.5h/);
  });
});

function rework(overrides: Partial<ReworkStat> = {}): ReworkStat {
  return {
    attempts_total: 0,
    attempts_excluded_stale: 0,
    attempts_with_rework_signal: 0,
    below_min_sample: true,
    rate: null,
    definition: 'def',
    truncation_note: 'note',
    ...overrides,
  };
}

describe('describeRework', () => {
  it('reports no eligible history when nothing is eligible', () => {
    expect(describeRework(rework())).toMatch(/no eligible dispatch history/);
  });

  it('shows raw counts below the minimum sample size, not a percentage', () => {
    const stat = rework({
      attempts_total: 3,
      attempts_excluded_stale: 0,
      attempts_with_rework_signal: 1,
      below_min_sample: true,
    });
    const out = describeRework(stat);
    expect(out).toMatch(/too few eligible attempts/);
    expect(out).toMatch(/1 of 3/);
    expect(out).not.toMatch(/%/);
  });

  it('shows a percentage once the minimum sample size is met', () => {
    const stat = rework({
      attempts_total: 10,
      attempts_excluded_stale: 0,
      attempts_with_rework_signal: 2,
      below_min_sample: false,
      rate: 0.2,
    });
    const out = describeRework(stat);
    expect(out).toMatch(/20\.0%/);
    expect(out).toMatch(/2 of 10/);
  });
});

describe('describeCostPerItem', () => {
  it('reports no agent-dispatched items distinctly from too-few-samples', () => {
    expect(describeCostPerItem(null, null, 0, 5)).toMatch(/no agent-dispatched items/);
  });

  it('withholds the figure below the minimum sample size, naming the count and minimum', () => {
    const out = describeCostPerItem(null, null, 3, 5);
    expect(out).toMatch(/too few agent-dispatched items/);
    expect(out).toMatch(/3/);
    expect(out).toMatch(/minimum 5/);
  });

  it('shows the estimated figure, still through formatEstimatedCost, once at/above the minimum', () => {
    const out = describeCostPerItem(1.5, null, 5, 5);
    expect(out).toMatch(/estimated/);
    expect(out).toMatch(/per item/);
  });
});
