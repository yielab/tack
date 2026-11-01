import { describe, it, expect } from 'vitest';
import { plainLine, plainVerdict } from './plainWords';

describe('plainVerdict', () => {
  it('says a verifier verdict in a sentence and names no verifier', () => {
    const text = plainVerdict('prill: human-review — review: criterion_manual — m1')!;
    expect(text).toBe('A person has to review it, and a check only a person can make (m1).');
    expect(text).not.toMatch(/prill|criterion_manual|human-review/);
  });

  it('says a file check, the next-steps heading, the ceiling and the approval line in plain sentences', () => {
    expect(plainVerdict('file_present')).toBe('A file the task needs is not there.');
    expect(plainVerdict('NEXT STEPS: blocking criterion impossible to satisfy')).toBe(
      'The checks cannot be met as they are written. The next steps say what to change.',
    );
    expect(plainVerdict("this pod's ceiling L3 (profile attended)")).toContain('no further than L3: you approve each step');
    expect(plainVerdict('approval required before lead hop (L5 asks nobody)')).toBe(
      'A person has to approve before the Lead step, and at L5 nobody is asked, so it cannot go on by itself.',
    );
  });

  it('leaves a line it does not know as it came', () => {
    expect(plainVerdict('Looks fine to me')).toBeNull();
    expect(plainLine('Looks fine to me')).toEqual({ text: 'Looks fine to me', raw: undefined });
  });

  it('keeps the raw line for a tooltip when it said it differently', () => {
    expect(plainLine('file_present')).toEqual({ text: 'A file the task needs is not there.', raw: 'file_present' });
  });
});
