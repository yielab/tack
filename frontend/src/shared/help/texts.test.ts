import { describe, it, expect } from 'vitest';
import { EGRESS_MODES, HELP, SANDBOX_TEXT, SAFETY_TEXT, agentSafety, defaultEgressMode, egressEnforced, egressHelp } from './texts';

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

  it('does not ask to install bubblewrap when it is installed but the computer refuses it', () => {
    const report = { harness_kind: 'claude-code', sandbox: 'none', sandbox_required: true, sandbox_refused: true, decisions: { support: 'supported' } } as const;
    expect(agentSafety([report])).toEqual({ text: SAFETY_TEXT.bwrapRefused, tone: 'blocked' });
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

describe('web access', () => {
  it('starts each kind of profile where the server does', () => {
    expect(['implementer', 'custom', undefined, 'reviewer', 'planner', 'researcher'].map(defaultEgressMode)).toEqual([
      'listed', 'listed', 'listed', 'closed', 'closed', 'open',
    ]);
  });

  it('gives each mode one short line and a help that names what none of them covers', () => {
    expect(EGRESS_MODES.map((m) => m.value)).toEqual(['closed', 'listed', 'open']);
    for (const mode of EGRESS_MODES) expect(mode.label).not.toContain('\n');
    expect(HELP.egressMode.text).toContain('can still receive what the agent sends it');
    expect(HELP.egressMode.text).toContain('private networks are refused');
  });

  it('says the mode is only asked for until the computer reports that it enforces it, then that it is enforced', () => {
    for (const unknown of [undefined, false]) {
      expect(egressHelp(unknown).text).toContain('only asked to follow it');
      expect(egressHelp(unknown).text).not.toContain('This computer enforces it');
    }
    const enforced = egressHelp(true);
    expect(enforced.text).toContain('This computer enforces it: the agent has no other way out');
    expect(enforced.text).not.toContain('only asked');
    expect(enforced.text).toContain('can still receive what the agent sends it');
    expect(enforced.href).toBe(HELP.egressMode.href);
  });

  it('holds the agent to its mode only when every computer it could land on says so', () => {
    expect(egressEnforced([])).toBeUndefined();
    expect(egressEnforced([{}])).toBeUndefined();
    expect(egressEnforced([{ egress: { enforced: true } }, {}])).toBe(true);
    expect(egressEnforced([{ egress: { enforced: true } }, { egress: { enforced: false } }])).toBe(false);
  });
});
