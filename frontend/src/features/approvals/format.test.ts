import { describe, it, expect } from 'vitest';
import { actionLabel, agentLabel, correlatedItemLabel, elapsedSince } from './format';
import type { PendingApproval } from './api';

function row(overrides: Partial<PendingApproval> = {}): PendingApproval {
  return {
    token: 'apr-1',
    control_plane_id: 'cp-1',
    control_plane_name: 'docket-1',
    item_id: null,
    item_title: null,
    item_status: null,
    project_id: null,
    project_name: null,
    remote_task_id: null,
    agent: null,
    action: null,
    requested_at: new Date().toISOString(),
    ...overrides,
  };
}

describe('elapsedSince', () => {
  it('renders "just now" for a timestamp within the last few seconds', () => {
    expect(elapsedSince(new Date().toISOString())).toBe('just now');
  });

  it('renders minutes for an older timestamp', () => {
    const iso = new Date(Date.now() - 5 * 60_000).toISOString();
    expect(elapsedSince(iso)).toBe('5m');
  });

  it('renders hours for a much older timestamp', () => {
    const iso = new Date(Date.now() - 3 * 3_600_000).toISOString();
    expect(elapsedSince(iso)).toBe('3h');
  });

  it('renders days for a very old timestamp', () => {
    const iso = new Date(Date.now() - 2 * 86_400_000).toISOString();
    expect(elapsedSince(iso)).toBe('2d');
  });

  it('degrades to "unknown" for an unparseable timestamp rather than NaN/blank', () => {
    expect(elapsedSince('not-a-date')).toBe('unknown');
  });
});

describe('correlatedItemLabel', () => {
  it('returns the item title when correlated', () => {
    expect(correlatedItemLabel(row({ item_id: 'i-1', item_title: 'Deploy service' }))).toBe(
      'Deploy service'
    );
  });

  it('names uncorrelated explicitly — never blank — for the row this whole inbox exists to surface', () => {
    const label = correlatedItemLabel(row({ item_id: null, item_title: null }));
    expect(label.toLowerCase()).toContain('uncorrelated');
  });
});

describe('agentLabel', () => {
  it('returns the agent when present', () => {
    expect(agentLabel(row({ agent: 'builder' }))).toBe('builder');
  });

  it('degrades honestly when agent is null or blank', () => {
    expect(agentLabel(row({ agent: null }))).toBe('unknown agent');
    expect(agentLabel(row({ agent: '  ' }))).toBe('unknown agent');
  });
});

describe('actionLabel', () => {
  it('returns the action text when present', () => {
    expect(actionLabel(row({ action: 'git push origin main' }))).toBe('git push origin main');
  });

  it('degrades honestly when action is null or blank, never blank', () => {
    expect(actionLabel(row({ action: null }))).not.toBe('');
    expect(actionLabel(row({ action: '' }))).not.toBe('');
  });
});
