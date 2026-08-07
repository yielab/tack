import { describe, it, expect } from 'vitest';
import { elapsedSince, HEALTH_LABEL, HEALTH_TONE, healthExplanation } from './format';

describe('elapsedSince', () => {
  it('renders just now for very recent timestamps', () => {
    expect(elapsedSince(new Date().toISOString())).toBe('just now');
  });

  it('renders seconds, minutes, hours, days as elapsed time grows', () => {
    const ago = (ms: number) => new Date(Date.now() - ms).toISOString();
    expect(elapsedSince(ago(30_000))).toBe('30s');
    expect(elapsedSince(ago(5 * 60_000))).toBe('5m');
    expect(elapsedSince(ago(3 * 3_600_000))).toBe('3h');
    expect(elapsedSince(ago(2 * 86_400_000))).toBe('2d');
  });

  it('falls back to "unknown" for an unparseable timestamp', () => {
    expect(elapsedSince('not-a-date')).toBe('unknown');
  });
});

describe('HEALTH_LABEL / HEALTH_TONE', () => {
  it('covers every ControlPlaneHealth value, including the fifth (unconfigured) one', () => {
    for (const h of ['healthy', 'degraded', 'unreachable', 'unknown', 'unconfigured'] as const) {
      expect(HEALTH_LABEL[h]).toBeTruthy();
      expect(HEALTH_TONE[h]).toBeTruthy();
    }
  });

  it('healthy is success tone, unreachable is danger tone', () => {
    expect(HEALTH_TONE.healthy).toBe('success');
    expect(HEALTH_TONE.unreachable).toBe('danger');
  });

  it('unconfigured has its own label, distinct from every other state', () => {
    const labels = ['healthy', 'degraded', 'unreachable', 'unknown', 'unconfigured'] as const;
    const distinct = new Set(labels.map((h) => HEALTH_LABEL[h]));
    expect(distinct.size).toBe(labels.length);
  });
});

describe('healthExplanation', () => {
  it('mentions the actual poll interval for the unknown state', () => {
    expect(healthExplanation('unknown', 15)).toContain('15s');
  });

  it('returns non-empty copy for every health value', () => {
    for (const h of ['healthy', 'degraded', 'unreachable', 'unknown', 'unconfigured'] as const) {
      expect(healthExplanation(h, 10).length).toBeGreaterThan(0);
    }
  });

  it('tells the operator how to recover from unconfigured — re-enter the token', () => {
    expect(healthExplanation('unconfigured', 10)).toMatch(/token/i);
  });
});
