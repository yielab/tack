import { describe, it, expect } from 'vitest';
import {
  formatEstimatedCost,
  formatTokens,
  relativeTime,
  deriveAgentChipState,
  eventTypeLabel,
  eventsTruncatedMessage,
} from './format';

describe('formatEstimatedCost', () => {
  it('always includes the literal word "estimated"', () => {
    expect(formatEstimatedCost(12.5, null)).toMatch(/estimated/);
    expect(formatEstimatedCost(0, '2026-07-01T00:00:00Z')).toMatch(/estimated/);
  });

  it('never renders a bare "$0.00" for a null (unknown) cost — says unavailable instead', () => {
    const out = formatEstimatedCost(null, null);
    expect(out).not.toMatch(/\$0\.00/);
    expect(out).toMatch(/unavailable/);
  });

  it('includes the pricing-snapshot date when known', () => {
    const out = formatEstimatedCost(9.99, '2026-07-15T00:00:00Z');
    expect(out).toMatch(/pricing as of/);
    expect(out).toMatch(/2026/);
  });

  it('never silently drops the pricing-snapshot qualifier when the date is null — states it is unknown', () => {
    // This is the B5 card's explicit correctness bar, stricter than the Fleet
    // view's equivalent helper (which omits the clause entirely when null):
    // "handle the null case honestly (don't print a fake date, and don't
    // silently drop the qualifier)."
    const out = formatEstimatedCost(9.99, null);
    expect(out).toMatch(/pricing snapshot date unknown/);
    expect(out).not.toMatch(/pricing as of/);
  });
});

describe('formatTokens', () => {
  it('renders small counts verbatim', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(842)).toBe('842');
  });

  it('renders thousands with a k suffix and millions with an M suffix', () => {
    expect(formatTokens(12_400)).toBe('12.4k');
    expect(formatTokens(3_200_000)).toBe('3.2M');
  });
});

describe('relativeTime', () => {
  it('reports "never" for a null timestamp', () => {
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

describe('deriveAgentChipState', () => {
  it('maps every known TaskStatus value to its chip state', () => {
    expect(deriveAgentChipState('pending')).toBe('queued');
    expect(deriveAgentChipState('running')).toBe('running');
    expect(deriveAgentChipState('waiting_approval')).toBe('waiting_approval');
    expect(deriveAgentChipState('done')).toBe('done');
    expect(deriveAgentChipState('failed')).toBe('failed');
  });

  it('folds "blocked" into the "failed" chip state (documented decision — no 6th slot)', () => {
    expect(deriveAgentChipState('blocked')).toBe('failed');
  });

  it('degrades an unrecognised remote_status to "failed" rather than erroring or looking healthy', () => {
    expect(deriveAgentChipState('some_future_docket_status')).toBe('failed');
    expect(() => deriveAgentChipState('')).not.toThrow();
  });
});

describe('eventTypeLabel', () => {
  it('gives a short label for a known event type', () => {
    expect(eventTypeLabel('tool_call')).toBe('Tool call');
    expect(eventTypeLabel('tester_verdict_failed')).toBe('Tester verdict: failed');
  });

  it('title-cases an unrecognised event type rather than dropping it (a docket upgrade must degrade to shown-as-is)', () => {
    expect(eventTypeLabel('some_new_event_type')).toBe('Some New Event Type');
  });
});

describe('eventsTruncatedMessage', () => {
  it('mentions the retention window so the caveat is actionable', () => {
    expect(eventsTruncatedMessage(90)).toMatch(/90-day/);
    expect(eventsTruncatedMessage(30)).toMatch(/30-day/);
  });

  it('says events "may" have been aged out — never a precise, unknowable count', () => {
    const out = eventsTruncatedMessage(90);
    expect(out).toMatch(/may have been aged out/);
    // The retention rollup drops item_id, so no count is ever computable —
    // guard against a future edit inventing a number like "N events".
    expect(out).not.toMatch(/\d+ events?\b/);
  });
});
