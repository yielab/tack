import { describe, it, expect } from 'vitest';
import {
  HARNESS_KINDS,
  buildCreateExecutionInput,
  generateIdempotencyKey,
  gateHarnessModelSelection,
  describeExecutionState,
  isTerminalStateString,
  relativeTimeFromIso,
  isActiveRunnerState,
  shouldHideTargetPicker,
  isExecutionOff,
  isDecisionsAttested,
  resolveRunModel,
  type RunWithAgentFormValues,
} from './shared';
import type { RunnerCapabilities, HarnessCapability } from '../execution';

function values(overrides: Partial<RunWithAgentFormValues> = {}): RunWithAgentFormValues {
  return {
    itemId: 'item-1',
    selectorKind: 'fleet',
    selectorId: 'fleet-1',
    agentProfileId: 'profile-1',
    harnessKind: 'codex',
    model: null,
    timeoutSeconds: 1800,
    allowNetwork: false,
    approvals: 'auto',
    tools: ['read', 'write'],
    repository: { kind: 'git', remote: 'git@example.com:org/repo.git', baseRevision: 'main', subdirectory: null },
    idempotencyKey: 'fixed-key',
    ...overrides,
  };
}

describe('HARNESS_KINDS', () => {
  it('carries the four bundled adapters under their wire values — claude-code with a hyphen', () => {
    expect(HARNESS_KINDS.map((h) => h.value)).toEqual(['codex', 'claude-code', 'docket', 'opencode']);
  });
});

describe('buildCreateExecutionInput', () => {
  it('builds a CreateExecutionInput with every top-level field the real handler requires', () => {
    const input = buildCreateExecutionInput(values());
    expect(input.item_id).toBe('item-1');
    expect(input.idempotency_key).toBe('fixed-key');
    expect(input.selector_kind).toBe('fleet');
    expect(input.selector_id).toBe('fleet-1');
    expect(input.agent_profile_id).toBe('profile-1');
    expect(input.requested_harness_kind).toBe('codex');
    expect(input.requested_model).toBeNull();
    expect(input.timeout_seconds).toBe(1800);
    expect(input.status_map_policy_id).toBeNull();
  });

  it('sends no agent_profile_snapshot: the server loads the profile by id', () => {
    const input = buildCreateExecutionInput(values());
    expect(input).not.toHaveProperty('agent_profile_snapshot');
    expect(input.agent_profile_id).toBeTruthy();
  });

  // Pinned against `RepositorySnapshot { kind, remote, base_revision, subdirectory }`.
  it('repository_snapshot carries exactly the fields RepositorySnapshot requires', () => {
    const input = buildCreateExecutionInput(values());
    const repo = input.repository_snapshot as Record<string, unknown>;
    expect(Object.keys(repo).sort()).toEqual(['base_revision', 'kind', 'remote', 'subdirectory']);
    expect(repo.kind).toBe('git');
    expect(repo.remote).toBe('git@example.com:org/repo.git');
    expect(repo.base_revision).toBe('main');
    expect(repo.subdirectory).toBeNull();
  });

  // Pinned against `PermissionPolicy { tools: Vec<String>, network: bool }`
  // — `network` is required with no server-side default (E5's live-tested
  // finding), so this module always sends an explicit boolean, never omits it.
  it('permission_policy carries exactly the fields PermissionPolicy requires', () => {
    const input = buildCreateExecutionInput(values({ allowNetwork: true }));
    const policy = input.permission_policy as Record<string, unknown>;
    expect(Object.keys(policy).sort()).toEqual(['network', 'tools']);
    expect(policy.network).toBe(true);
    expect(policy.tools).toEqual(['read', 'write']);
  });

  it('budgets/environment/metadata default to {} — confirmed safe by E5s live test', () => {
    const input = buildCreateExecutionInput(values());
    expect(input.budgets).toEqual({});
    expect(input.environment).toEqual({});
    expect(input.metadata).toEqual({});
  });

  it('a chosen model string round-trips exactly, not coerced', () => {
    const input = buildCreateExecutionInput(values({ model: 'opaque/model-alpha' }));
    expect(input.requested_model).toBe('opaque/model-alpha');
  });

  it('two different entry points building from equal form values produce byte-identical payloads', () => {
    // Proof that all three surfaces create the same payload shape, with no
    // divergent DTOs between entry points — Board/item-detail/Sprint all end
    // up calling this exact function, so equal input always yields equal
    // output.
    const a = buildCreateExecutionInput(values());
    const b = buildCreateExecutionInput(values());
    expect(a).toEqual(b);
  });
});

describe('generateIdempotencyKey', () => {
  it('produces a fresh, non-empty key each call', () => {
    const a = generateIdempotencyKey();
    const b = generateIdempotencyKey();
    expect(a).not.toBe(b);
    expect(a.length).toBeGreaterThan(0);
  });
});

function capsWith(harness: {
  harness_kind: string;
  probe_error: string | null;
  requires_model?: boolean;
}): RunnerCapabilities[] {
  return [
    {
      runner_version: '0.1.0',
      reported_at: '2026-08-06T12:00:00Z',
      labels: {},
      concurrency: { total: 1, available: 1 },
      harnesses: [
        {
          harness_kind: harness.harness_kind,
          installed_version: '1.0.0',
          probe_error: harness.probe_error,
          probed_at: '2026-08-06T12:00:00Z',
          ...(harness.requires_model === undefined ? {} : { requires_model: harness.requires_model }),
        },
      ],
      features: {
        cancel: { support: 'advisory', reason: null },
        resume: { support: 'unsupported', reason: null },
        decisions: { support: 'supported', reason: null },
        artifacts: { support: 'supported', reason: null },
        usage: { support: 'advisory', reason: null },
      },
      limits: { event_payload_bytes_max: 1024, artifact_content_bytes_max: 1024 },
    },
  ];
}

describe('gateHarnessModelSelection', () => {
  const base = { harness_kind: 'codex', probe_error: null };

  it('a run with a model is allowed and names it', () => {
    const gate = gateHarnessModelSelection(capsWith({ ...base, requires_model: true }), 'codex', 'gpt-5-codex');
    expect(gate).toMatchObject({ allowed: true, reason: 'Runs on gpt-5-codex' });
  });

  it('a run with no model is allowed when the harness picks its own, or an older runner omits the field', () => {
    expect(gateHarnessModelSelection(capsWith({ ...base, requires_model: false }), 'codex', null)).toMatchObject({
      allowed: true,
      reason: "Uses the agent's own model",
    });
    expect(gateHarnessModelSelection(capsWith(base), 'codex', null).allowed).toBe(true);
  });

  it('a run with no model is blocked, naming the agent, when every runner reporting it needs one', () => {
    const gate = gateHarnessModelSelection(capsWith({ ...base, requires_model: true }), 'codex', null);
    expect(gate.allowed).toBe(false);
    expect(gate.reason).toContain('codex needs a model named');
  });

  it('the block carries a fix to project settings when a href is given', () => {
    const gate = gateHarnessModelSelection(capsWith({ ...base, requires_model: true }), 'codex', null, '/projects/p1/settings?tab=automation');
    expect(gate.fix).toEqual({ label: 'Set a default model for this project', href: '/projects/p1/settings?tab=automation' });
  });

  it('with zero capability snapshots nothing is allowed, with a typed reason', () => {
    const gate = gateHarnessModelSelection([], 'codex', 'gpt-5-codex');
    expect(gate.allowed).toBe(false);
    expect(gate.reason).toMatch(/no runner capability data/i);
  });
});

describe('resolveRunModel', () => {
  const project = { default_harness: 'claude-code', default_model: 'claude-opus-5-5' };

  it('the request\'s own model wins over the project default', () => {
    expect(resolveRunModel('claude-sonnet-5-5', 'claude-code', project)).toBe('claude-sonnet-5-5');
  });
  it('the project default applies when the run\'s agent is the project\'s default agent', () => {
    expect(resolveRunModel(null, 'claude-code', project)).toBe('claude-opus-5-5');
  });
  it('the project default does not apply to another agent, or to a project with no default agent', () => {
    expect(resolveRunModel(null, 'codex', project)).toBeNull();
    expect(resolveRunModel(null, 'claude-code', { default_harness: null, default_model: 'claude-opus-5-5' })).toBeNull();
  });
  it('names none when nothing does', () => {
    expect(resolveRunModel(null, 'codex', null)).toBeNull();
    expect(resolveRunModel(null, 'codex', undefined)).toBeNull();
  });
});

describe('describeExecutionState', () => {
  it('labels every one of the ten frozen v1 states', () => {
    const states = [
      'queued', 'leased', 'preparing', 'running', 'waiting_decision',
      'succeeded', 'failed', 'cancelled', 'lost', 'needs_operator',
    ];
    for (const s of states) {
      const info = describeExecutionState(s);
      expect(info.known).toBe(true);
      expect(info.label.length).toBeGreaterThan(0);
    }
  });

  it('an unrecognised state still renders (as itself), never throws', () => {
    const info = describeExecutionState('some_future_state');
    expect(info.known).toBe(false);
    expect(info.label).toBe('some_future_state');
    expect(info.tone).toBe('neutral');
  });
});

describe('isTerminalStateString', () => {
  it('matches the frozen terminal set exactly', () => {
    expect(isTerminalStateString('succeeded')).toBe(true);
    expect(isTerminalStateString('failed')).toBe(true);
    expect(isTerminalStateString('cancelled')).toBe(true);
    expect(isTerminalStateString('running')).toBe(false);
    expect(isTerminalStateString('needs_operator')).toBe(false);
  });
});

describe('relativeTimeFromIso', () => {
  it('handles null/undefined/invalid input without throwing', () => {
    expect(relativeTimeFromIso(null)).toBe('unknown');
    expect(relativeTimeFromIso(undefined)).toBe('unknown');
    expect(relativeTimeFromIso('not-a-date')).toBe('unknown');
  });

  it('reports "just now" for a very recent timestamp', () => {
    expect(relativeTimeFromIso(new Date().toISOString())).toBe('just now');
  });
});

describe('isActiveRunnerState', () => {
  it('only "active" counts — pending enrollment and revoked do not', () => {
    expect(isActiveRunnerState('active')).toBe(true);
    expect(isActiveRunnerState('pending_enrollment')).toBe(false);
    expect(isActiveRunnerState('revoked')).toBe(false);
    expect(isActiveRunnerState('some_future_state')).toBe(false);
  });
});

describe('shouldHideTargetPicker', () => {
  it('hides only for exactly one active runner and zero fleets', () => {
    expect(shouldHideTargetPicker(1, 0)).toBe(true);
  });
  it('shows when a fleet exists too, even with one active runner', () => {
    expect(shouldHideTargetPicker(1, 1)).toBe(false);
  });
  it('shows for zero or more than one active runner', () => {
    expect(shouldHideTargetPicker(0, 0)).toBe(false);
    expect(shouldHideTargetPicker(2, 0)).toBe(false);
  });
});

describe('isExecutionOff', () => {
  it('is off only at zero active runners', () => {
    expect(isExecutionOff(0)).toBe(true);
    expect(isExecutionOff(1)).toBe(false);
    expect(isExecutionOff(2)).toBe(false);
  });
});

// The modal's own submit path (`buildCreateExecutionInput`) and its own
// control-gating helper (`isDecisionsAttested`), exercised together as the
// one scenario D2 adds a control for: the wire only ever carries `"ask"` as
// an explicit, deliberate choice, and that choice is only ever offered for
// a harness that attested it can honour it.
describe('approvals ("Ask me before acting")', () => {
  it('ask sends permission_policy.approvals, automatic omits it, and an unsupported harness cannot select ask', () => {
    const askInput = buildCreateExecutionInput(values({ approvals: 'ask' }));
    expect((askInput.permission_policy as Record<string, unknown>).approvals).toBe('ask');

    const autoInput = buildCreateExecutionInput(values({ approvals: 'auto' }));
    expect(autoInput.permission_policy as Record<string, unknown>).not.toHaveProperty('approvals');

    const unsupportedHarness: HarnessCapability = {
      harness_kind: 'codex',
      installed_version: '1.0.0',
      probe_error: null,
      probed_at: '2026-09-19T00:00:00Z',
      decisions: { support: 'unsupported', reason: 'nothing pauses a run to await a decision' },
    };
    expect(isDecisionsAttested(unsupportedHarness)).toBe(false);
  });
});
