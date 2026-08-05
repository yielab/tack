import { describe, it, expect } from 'vitest';
import {
  formatEstimatedCost,
  formatBudget,
  formatTokens,
  relativeTime,
  isStale,
  HEALTH_LABEL,
  HEALTH_TONE,
} from './format';

describe('formatEstimatedCost', () => {
  it('always includes the literal word "estimated"', () => {
    expect(formatEstimatedCost(12.5, null)).toMatch(/estimated/);
    expect(formatEstimatedCost(0, null)).toMatch(/estimated/);
    expect(formatEstimatedCost(1234.567, '2026-07-01T00:00:00Z')).toMatch(/estimated/);
  });

  it('never renders the word "spend" (docket cost figures are estimates, not billed spend)', () => {
    expect(formatEstimatedCost(12.5, '2026-07-01T00:00:00Z')).not.toMatch(/spend/i);
  });

  it('includes the pricing-snapshot date when known', () => {
    const out = formatEstimatedCost(9.99, '2026-07-15T00:00:00Z');
    expect(out).toMatch(/pricing as of/);
    expect(out).toMatch(/2026/);
  });

  it('omits the snapshot clause when the date is unknown', () => {
    const out = formatEstimatedCost(9.99, null);
    expect(out).not.toMatch(/pricing as of/);
  });

  it('never renders a bare "$0.00" for a null (unknown) cost — says unavailable instead', () => {
    const out = formatEstimatedCost(null, null);
    expect(out).not.toMatch(/\$0\.00/);
    expect(out).toMatch(/unavailable/);
  });

  it('formats a genuine zero cost as an explicit estimated $0.00, not a blank', () => {
    const out = formatEstimatedCost(0, null);
    expect(out).toMatch(/\$0\.00/);
    expect(out).toMatch(/estimated/);
  });
});

describe('formatBudget', () => {
  it('reports "no budget cap set" for null rather than a bare number', () => {
    expect(formatBudget(null)).toMatch(/no budget cap set/);
  });

  it('formats a set budget as currency', () => {
    expect(formatBudget(500)).toBe('$500.00');
  });
});

describe('formatTokens', () => {
  it('renders small counts verbatim', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(842)).toBe('842');
  });

  it('renders thousands with a k suffix', () => {
    expect(formatTokens(12_400)).toBe('12.4k');
  });

  it('renders millions with an M suffix', () => {
    expect(formatTokens(3_200_000)).toBe('3.2M');
  });
});

describe('relativeTime', () => {
  it('reports "never" for a null timestamp, not a blank or a stale-looking date', () => {
    expect(relativeTime(null)).toBe('never');
  });

  it('reports "unknown" for an unparsable timestamp', () => {
    expect(relativeTime('not-a-date')).toBe('unknown');
  });

  it('reports minutes ago for a recent timestamp', () => {
    const fourMinAgo = new Date(Date.now() - 4 * 60 * 1000).toISOString();
    expect(relativeTime(fourMinAgo)).toBe('4m ago');
  });
});

describe('isStale', () => {
  it('treats unreachable and unknown as stale', () => {
    expect(isStale('unreachable')).toBe(true);
    expect(isStale('unknown')).toBe(true);
  });

  it('treats healthy and degraded as not stale', () => {
    expect(isStale('healthy')).toBe(false);
    expect(isStale('degraded')).toBe(false);
  });
});

describe('HEALTH_LABEL / HEALTH_TONE', () => {
  it('covers every health state with a distinct tone', () => {
    const tones = new Set(Object.values(HEALTH_TONE));
    expect(tones.size).toBe(4); // healthy, degraded, unreachable, unknown are all visually distinct
    expect(Object.keys(HEALTH_LABEL).sort()).toEqual(
      ['degraded', 'healthy', 'unknown', 'unreachable'].sort(),
    );
  });
});
