import { describe, it, expect } from 'vitest';
import { SANDBOX_TEXT, SAFETY_TEXT, agentSafety } from './texts';

describe('agentSafety', () => {
  it('names who contains each of the four agents', () => {
    expect(agentSafety([{ harness_kind: 'claude-code', sandbox: 'bwrap', sandbox_required: true }])).toEqual({ text: SAFETY_TEXT.byComputer, tone: 'ok' });
    expect(agentSafety([{ harness_kind: 'opencode', sandbox: 'bwrap', sandbox_required: true }])).toEqual({ text: SAFETY_TEXT.byComputer, tone: 'ok' });
    expect(agentSafety([{ harness_kind: 'codex', sandbox: 'bwrap', sandbox_required: true }])).toEqual({ text: SAFETY_TEXT.byComputerAndAgent, tone: 'ok' });
    expect(agentSafety([{ harness_kind: 'docket', sandbox: 'harness' }])).toEqual({ text: SAFETY_TEXT.byDocket, tone: 'ok' });
    expect(agentSafety([{ sandbox: 'harness' }])).toEqual({ text: SAFETY_TEXT.byAgent, tone: 'ok' });
  });

  it('says an agent that must be contained does not run without bubblewrap', () => {
    const report = { harness_kind: 'claude-code', sandbox: 'none', sandbox_required: true, decisions: { support: 'supported' } } as const;
    expect(agentSafety([report])).toEqual({ text: SAFETY_TEXT.needsBubblewrap, tone: 'blocked' });
  });

  it('says when the administrator turned the requirement off, and that docket never blocks', () => {
    const asks = { support: 'supported' };
    expect(agentSafety([{ harness_kind: 'opencode', sandbox: 'none', sandbox_required: false, decisions: asks }])).toEqual({ text: SAFETY_TEXT.optedOut, tone: 'asks' });
    expect(agentSafety([{ harness_kind: 'docket', sandbox: 'none', decisions: asks }])).toEqual({ text: SAFETY_TEXT.docketSilent, tone: 'asks' });
  });

  it('lets the least safe machine decide', () => {
    expect(agentSafety([{ sandbox: 'harness' }, { sandbox: 'none', decisions: { support: 'supported' } }])?.tone).toBe('asks');
    expect(agentSafety([{ sandbox: 'harness' }, { sandbox: 'none' }])?.tone).toBe('blocked');
  });

  it('says nothing when no machine reports a sandbox', () => {
    expect(agentSafety([{}])).toBeNull();
  });

  it('puts the agent\'s confinement in plain words', () => {
    expect(SANDBOX_TEXT.harness).toBe('Runs in an isolated sandbox');
  });
});
