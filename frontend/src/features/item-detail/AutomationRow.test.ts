import { describe, it, expect } from 'vitest';
import { automationSentence, choiceSource, withProfile } from './AutomationRow';

const base = { startsAt: 'To Do', starts: true, team: 'saas-content', merge: 'L4', roleWithoutProfile: null, hasProfile: true };

describe('automationSentence', () => {
  it('says when it starts, which team and how far it merges', () => {
    expect(automationSentence(base)).toBe('Starts by itself when it reaches To Do · Team saas-content · Merge L4');
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

describe('a task with nowhere to run', () => {
  it('says why it waits for a person, instead of promising it starts by itself', () => {
    const noPlace = '2 computers could run it and its profile names no group of computers to choose from';
    expect(automationSentence({ ...base, noPlace })).toBe(`By hand: it would start when it reaches To Do, but ${noPlace} · Team saas-content · Merge L4`);
  });
  it('keeps the other reasons first', () => {
    expect(automationSentence({ ...base, starts: false, noPlace: 'x' })).toBe("By hand: this project doesn't start tasks by itself");
  });
});

describe('a task auto-start has not started yet', () => {
  it('says why, right after when it starts', () => {
    expect(automationSentence({ ...base, waiting: '3 runs are already open, the most this project allows' })).toBe(
      'Starts by itself when it reaches To Do · Waiting to start: 3 runs are already open, the most this project allows · Team saas-content · Merge L4',
    );
  });
  it('says nothing when nothing holds it back, and gives way to the reasons it is by hand', () => {
    expect(automationSentence({ ...base, waiting: null })).toBe('Starts by itself when it reaches To Do · Team saas-content · Merge L4');
    expect(automationSentence({ ...base, starts: false, waiting: 'x' })).toBe("By hand: this project doesn't start tasks by itself");
    expect(automationSentence({ ...base, noPlace: 'no computer', waiting: 'x' })).not.toContain('Waiting to start');
  });
});

describe('who does it', () => {
  it('names the profile and where the choice came from', () => {
    expect(automationSentence({ ...base, who: 'Content writer (chosen on this task)' })).toBe(
      'Starts by itself when it reaches To Do · Goes to Content writer (chosen on this task) · Team saas-content · Merge L4',
    );
  });
  it('says the choice was made on the task, or by the role', () => {
    expect(choiceSource({ onTask: true, role: 'Content' })).toBe('chosen on this task');
    expect(choiceSource({ onTask: false, role: 'Content' })).toBe('from the role Content');
    expect(choiceSource({ onTask: false, role: null })).toBeNull();
  });
  it('sets the profile in the run settings and keeps the rest', () => {
    expect(withProfile({ timeout_seconds: 900, later_key: 1 }, 'p1')).toEqual({ timeout_seconds: 900, later_key: 1, agent_profile_id: 'p1' });
    expect(withProfile(null, 'p1')).toEqual({ agent_profile_id: 'p1' });
  });
  it('clears the profile, and the settings when nothing else is left', () => {
    expect(withProfile({ agent_profile_id: 'p1', timeout_seconds: 900 }, null)).toEqual({ timeout_seconds: 900 });
    expect(withProfile({ agent_profile_id: 'p1' }, null)).toBeNull();
  });
});
