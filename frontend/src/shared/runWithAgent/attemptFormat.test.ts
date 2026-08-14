import { describe, it, expect } from 'vitest';
import {
  NOT_MEASURED_TEXT,
  describeModelProvenance,
  formatRunnerTimeCost,
  formatUsageEconomics,
  formatUsdMeasurement,
  formatWallClock,
} from './attemptFormat';

describe('formatUsdMeasurement — "Not measured" must be exact for absent usage', () => {
  it('renders the literal "Not measured" for source: not_measured, never $0.00 / — / blank', () => {
    const text = formatUsdMeasurement({ value: null, source: 'not_measured' });
    expect(text).toBe('Not measured');
    expect(text).toBe(NOT_MEASURED_TEXT);
    expect(text).not.toContain('$0');
    expect(text).not.toBe('—');
    expect(text).not.toBe('');
  });

  it('defensively treats a null value as "Not measured" even if source somehow disagrees', () => {
    // The backend guarantees these are always paired, but this function does
    // not trust that pairing blindly (see its own doc comment) — a null
    // value never renders as a number regardless of what `source` claims.
    expect(formatUsdMeasurement({ value: null, source: 'measured' })).toBe('Not measured');
  });

  it('a real measured zero is a genuine, distinct fact — never collapsed into "Not measured"', () => {
    const text = formatUsdMeasurement({ value: 0, source: 'measured' });
    expect(text).toBe('$0.00 (measured)');
    expect(text).not.toBe('Not measured');
  });

  it('renders a real measured value with its provenance label', () => {
    expect(formatUsdMeasurement({ value: 1.5, source: 'measured' })).toBe('$1.50 (measured)');
  });

  it('renders a real estimated value with its provenance label', () => {
    expect(formatUsdMeasurement({ value: 2.4956, source: 'estimated' })).toBe('$2.50 (estimated)');
  });

  it('uses more precision for a small nonzero value so it never rounds down to $0.00', () => {
    const text = formatUsdMeasurement({ value: 0.0021, source: 'estimated' });
    expect(text).toBe('$0.0021 (estimated)');
    expect(text).not.toBe('$0.00 (estimated)');
  });
});

describe('formatWallClock', () => {
  it('renders a distinct "Unknown" message for null — never the same text as "Not measured"', () => {
    const text = formatWallClock(null);
    expect(text).toContain('Unknown');
    expect(text).not.toBe(NOT_MEASURED_TEXT);
  });

  it('formats seconds only under a minute', () => {
    expect(formatWallClock(45_000)).toBe('45s');
  });

  it('formats minutes and seconds', () => {
    expect(formatWallClock(295_000)).toBe('4m 55s');
  });

  it('formats hours, minutes and seconds', () => {
    expect(formatWallClock(3_725_000)).toBe('1h 2m 5s');
  });
});

describe('formatRunnerTimeCost / formatUsageEconomics — never summed, always two line items', () => {
  it('reports both dimensions as "Not measured" for the real-world every-response-today case', () => {
    const usage = {
      model_token_cost_usd_estimated: { value: null, source: 'not_measured' as const },
      runner_time_cost: { wall_clock_ms: null, cost_usd_estimated: { value: null, source: 'not_measured' as const } },
    };
    const display = formatUsageEconomics(usage);
    expect(display.modelTokenCostUsd).toBe('Not measured');
    expect(display.runnerTime.costUsd).toBe('Not measured');
    expect(display.runnerTime.wallClock).toContain('Unknown');
  });

  it('keeps the two dollar dimensions independently provenanced, never equal by accident', () => {
    const cost = formatRunnerTimeCost({
      wall_clock_ms: 1_800_000, // 30 minutes
      cost_usd_estimated: { value: 1.5, source: 'estimated' },
    });
    expect(cost.wallClock).toBe('30m 0s');
    expect(cost.costUsd).toBe('$1.50 (estimated)');

    const usage = formatUsageEconomics({
      model_token_cost_usd_estimated: { value: 0.42, source: 'measured' },
      runner_time_cost: { wall_clock_ms: 1_800_000, cost_usd_estimated: { value: 1.5, source: 'estimated' } },
    });
    expect(usage.modelTokenCostUsd).not.toBe(usage.runnerTime.costUsd);
  });
});

describe('describeModelProvenance', () => {
  it('a null provenance ("not yet reported") is worded distinctly from "Not measured"', () => {
    const d = describeModelProvenance(null);
    expect(d.label).toBe('Not yet reported');
    expect(d.tone).toBe('neutral');
    expect(d.detail).not.toContain(NOT_MEASURED_TEXT);
  });

  it('matched renders a success tone and names the real provider/model', () => {
    const d = describeModelProvenance({ kind: 'matched', provider: 'openai', model_id: 'opaque/model-alpha' });
    expect(d.tone).toBe('success');
    expect(d.detail).toContain('openai');
    expect(d.detail).toContain('opaque/model-alpha');
  });

  it('auto_select_observed renders an info tone', () => {
    const d = describeModelProvenance({ kind: 'auto_select_observed', actual_provider: 'anthropic', actual_model_id: 'opaque/model-beta' });
    expect(d.tone).toBe('info');
    expect(d.detail).toContain('anthropic');
  });

  it('mismatched shows both requested and actual values, never silently reconciled, with a warning tone', () => {
    const d = describeModelProvenance({
      kind: 'mismatched',
      requested_provider: 'openai',
      requested_model_id: 'opaque/model-alpha',
      actual_provider: 'anthropic',
      actual_model_id: 'opaque/model-beta',
    });
    expect(d.tone).toBe('warning');
    expect(d.detail).toContain('openai');
    expect(d.detail).toContain('opaque/model-alpha');
    expect(d.detail).toContain('anthropic');
    expect(d.detail).toContain('opaque/model-beta');
  });
});
