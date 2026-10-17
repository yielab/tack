import { describe, it, expect } from 'vitest';
import { SANDBOX_TEXT, SAFETY_TEXT, agentSafety } from './texts';

describe('agentSafety', () => {
  it('reads an agent that confines itself like a sandboxed one', () => {
    expect(agentSafety([{ sandbox: 'harness' }])).toEqual({ text: SAFETY_TEXT.ok, tone: 'ok' });
  });

  it('lets the least safe machine decide', () => {
    expect(agentSafety([{ sandbox: 'harness' }, { sandbox: 'none', decisions: { support: 'supported' } }])?.tone).toBe('asks');
    expect(agentSafety([{ sandbox: 'harness' }, { sandbox: 'none' }])?.tone).toBe('blocked');
  });

  it('says nothing when no machine reports a sandbox', () => {
    expect(agentSafety([{}])).toBeNull();
  });

  it('puts docket\'s own confinement in words', () => {
    expect(SANDBOX_TEXT.harness).toBe('Sandboxed by docket in its own jail');
  });
});
