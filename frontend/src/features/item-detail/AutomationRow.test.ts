import { describe, it, expect } from 'vitest';
import { automationSentence } from './AutomationRow';

const base = { startsAt: 'To Do', starts: true, team: 'fab-content', merge: 'L4', roleWithoutProfile: null, hasProfile: true };

describe('automationSentence', () => {
  it('says when it starts, which team and how far it merges', () => {
    expect(automationSentence(base)).toBe('Starts by itself when it reaches To Do · Team fab-content · Merge L4');
  });
  it('leaves out a team the profile does not name', () => {
    expect(automationSentence({ ...base, team: null })).toBe('Starts by itself when it reaches To Do · Merge L4');
  });
  it('says which role has no profile', () => {
    expect(automationSentence({ ...base, hasProfile: false, roleWithoutProfile: 'Content' })).toBe('By hand: the role Content has no profile');
  });
  it('says so when the project does not start tasks by itself', () => {
    expect(automationSentence({ ...base, starts: false })).toBe("By hand: this project doesn't start tasks by itself");
  });
});
