import { describe, it, expect } from 'vitest';
import { teamInitials } from './teamInitials';
import { hueFromName } from './Avatar';

describe('teamInitials', () => {
  it('reads a hyphenated name as words: the first letter of the first and of the last', () => {
    expect(teamInitials('saas-code')).toBe('SC');
    expect(teamInitials('saas-lights')).toBe('SL');
    expect(teamInitials('acme-content-gen')).toBe('AG');
    expect(teamInitials('my_team.alpha')).toBe('MA');
    expect(teamInitials('Content Ops')).toBe('CO');
  });

  it('reads one word as its first two letters, and nothing as a question mark', () => {
    expect(teamInitials('docs')).toBe('DO');
    expect(teamInitials('x')).toBe('X');
    expect(teamInitials('')).toBe('?');
    expect(teamInitials('---')).toBe('?');
  });

  it('gives the same initials for the same name whatever else is on the screen', () => {
    // The old helper read saas-code as SA alone and as CO beside saas-content; now only the name decides.
    const first = teamInitials('saas-code');
    for (let i = 0; i < 3; i += 1) expect(teamInitials('saas-code')).toBe(first);
  });

  it('leaves two teams that read alike to be told apart by their colour', () => {
    expect(teamInitials('saas-code')).toBe(teamInitials('saas-content'));
    expect(hueFromName('saas-code')).not.toBe(hueFromName('saas-content'));
  });
});
