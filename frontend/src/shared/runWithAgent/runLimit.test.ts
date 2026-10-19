import { describe, expect, it } from 'vitest';
import { doubled, isNearLimit, stoppedByLimit } from './runLimit';

describe('a run near its limit', () => {
  it('turns amber from 80% of the limit, and never without one', () => {
    expect(isNearLimit(799, 1000)).toBe(false);
    expect(isNearLimit(800, 1000)).toBe(true);
    expect(isNearLimit(1200, 1000)).toBe(true);
    expect(isNearLimit(800, null)).toBe(false);
    expect(isNearLimit(null, 1000)).toBe(false);
  });

  it('is stopped by its limit when the runner or docket says so', () => {
    expect(stoppedByLimit({ code: 'token_limit' })).toBe(true);
    expect(stoppedByLimit({ code: 'budget' })).toBe(true);
    expect(stoppedByLimit({ code: 'failed' })).toBe(false);
    expect(stoppedByLimit(null)).toBe(false);
  });

  it('runs again with twice the limit', () => {
    expect(doubled(720_000)).toBe(1_440_000);
  });
});
