import { describe, it, expect } from 'vitest';
import {
  gate,
  gatePause,
  gateResume,
  gateModelSelection,
  gateDecisions,
  gateUsage,
  gateEventScope,
  type Capabilities,
} from './capabilities';

/** A realistic docket `Capabilities` payload — the exact reasons
 *  `crates/tack-orch/src/adapters/docket.rs::capabilities()` returns, so a
 *  test failure here is a real signal, not a fixture drifting from the
 *  backend it mirrors. */
const docketCapabilities: Capabilities = {
  dispatch: true,
  cancel: false,
  pause: {
    level: 'unsupported',
    reason:
      'docket exposes no pause endpoint over HTTP in either direction; from the docket ' +
      'CLI, run `docket profile <pod-id> --resume` to clear a budget-triggered pause',
  },
  resume: {
    level: 'unsupported',
    reason:
      'docket exposes no resume endpoint over HTTP in either direction; from the docket ' +
      'CLI, run `docket profile <pod-id> --resume`',
  },
  event_scope: {
    level: 'project',
    reason:
      "docket's trace stream (GET /traces/{project}) is scoped per project; individual " +
      'events carry no run id to narrow further',
  },
  artifacts: false,
  decisions: {
    level: 'poll',
    reason:
      "pending approvals are read via GET /approvals on the reconciler's poll cadence; " +
      'docket has no push/webhook path for a new approval',
  },
  usage: {
    level: 'from_provider',
    reason:
      'docket estimates cost/token usage itself and reports it via /status.json, ' +
      '/metrics, and trace events; there is no metering gateway in front of it',
  },
  model_selection: {
    level: 'unsupported',
    reason:
      'docket owns its own model routing per role/blueprint and has no HTTP input to ' +
      'override it per task; a caller-supplied model would be silently ignored',
  },
  runtimes: true,
  plane_metrics: true,
  provisioning: true,
};

describe('gate', () => {
  it('disables the control only at the named offLevel, and always carries the reason through', () => {
    const g = gate({ level: 'unsupported', reason: 'nope' }, 'unsupported');
    expect(g.enabled).toBe(false);
    expect(g.reason).toBe('nope');
  });

  it('enables the control for any level other than offLevel', () => {
    const g = gate({ level: 'advisory', reason: 'best effort' }, 'unsupported');
    expect(g.enabled).toBe(true);
    // The reason is carried through even when enabled, so a caller can show
    // it as a caveat rather than only on the disabled path.
    expect(g.reason).toBe('best effort');
  });
});

describe('gatePause / gateResume — the required regression: a real reason, not a guess', () => {
  it("renders docket's real, adapter-authored reason for an Unsupported pause capability", () => {
    const g = gatePause(docketCapabilities);
    expect(g.enabled).toBe(false);
    // Assert on the actual TEXT, not just the disabled flag — a hard-coded
    // client-side string ("docket does not support pausing," say) would
    // also make `enabled` false and would defeat the point of this test.
    expect(g.reason).toContain('docket profile <pod-id> --resume');
    expect(g.reason).toBe(docketCapabilities.pause.reason);
  });

  it("renders docket's real reason for an Unsupported resume capability, distinct from pause's", () => {
    const g = gateResume(docketCapabilities);
    expect(g.enabled).toBe(false);
    expect(g.reason).toBe(docketCapabilities.resume.reason);
    expect(g.reason).not.toBe(docketCapabilities.pause.reason);
  });
});

describe('gateModelSelection', () => {
  it('is disabled for docket (Unsupported) with a reason naming why a supplied model would be ignored', () => {
    const g = gateModelSelection(docketCapabilities);
    expect(g.enabled).toBe(false);
    expect(g.reason).toContain('silently ignored');
  });

  it('is enabled at Advisory or Honoured — a future adapter that forwards the model', () => {
    expect(gateModelSelection({ ...docketCapabilities, model_selection: { level: 'advisory', reason: 'r' } }).enabled).toBe(true);
    expect(gateModelSelection({ ...docketCapabilities, model_selection: { level: 'honoured', reason: 'r' } }).enabled).toBe(true);
  });
});

describe('gateDecisions', () => {
  it('is enabled for docket (Poll)', () => {
    expect(gateDecisions(docketCapabilities).enabled).toBe(true);
  });

  it('is disabled only at None', () => {
    expect(gateDecisions({ ...docketCapabilities, decisions: { level: 'none', reason: 'no decision store' } }).enabled).toBe(false);
    expect(gateDecisions({ ...docketCapabilities, decisions: { level: 'push', reason: 'r' } }).enabled).toBe(true);
  });
});

describe('gateUsage', () => {
  it('is enabled for docket (FromProvider)', () => {
    expect(gateUsage(docketCapabilities).enabled).toBe(true);
  });

  it('is disabled only at NotMeasured, enabled at FromGateway too', () => {
    expect(gateUsage({ ...docketCapabilities, usage: { level: 'not_measured', reason: 'r' } }).enabled).toBe(false);
    expect(gateUsage({ ...docketCapabilities, usage: { level: 'from_gateway', reason: 'r' } }).enabled).toBe(true);
  });
});

describe('gateEventScope', () => {
  it('is enabled for docket (Project)', () => {
    expect(gateEventScope(docketCapabilities).enabled).toBe(true);
  });

  it('is disabled only at None', () => {
    expect(gateEventScope({ ...docketCapabilities, event_scope: { level: 'none', reason: 'r' } }).enabled).toBe(false);
  });
});
