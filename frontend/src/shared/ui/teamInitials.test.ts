import { describe, it, expect } from 'vitest';
import { teamInitials, teamInitialsOf } from './teamInitials';

describe('teamInitialsOf', () => {
  it('reads teams that share a prefix after it and never shows one pair twice', () => {
    const m = teamInitialsOf(['saas-code', 'saas-content', 'saas-lights']);
    expect(m.get('saas-lights')).toBe('LI');
    expect(new Set(m.values()).size).toBe(3);
    expect([...m.values()].every((v) => !v.startsWith('SA'))).toBe(true);
  });

  it('uses the first letters of the first two words when the rest has several', () => {
    expect(teamInitialsOf(['acme-content-gen', 'acme-ops']).get('acme-content-gen')).toBe('CG');
  });

  it('falls back to the name as it always read when there is no shared prefix', () => {
    expect(teamInitialsOf(['alpha', 'beta']).get('alpha')).toBe('AL');
    expect(teamInitials('saas-code', [])).toBe('SA');
  });

  it('keeps a team whose name is the prefix of another readable', () => {
    expect(teamInitialsOf(['saas', 'saas-code']).get('saas')).toBe('SA');
  });
});
