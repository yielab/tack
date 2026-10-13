import { describe, it, expect } from 'vitest';
import {
  gateFeature,
  gateFeatureAcrossRunners,
  listReportedHarnessKinds,
  harnessProbeStatus,
} from './capabilities';
import type { RunnerCapabilities } from './types';

// Fixture shaped exactly like docs/contracts/runner-v1/capabilities.json
// (III.1.6's frozen authority), so this test exercises real wire shapes,
// not an invented convenience shape.
function fixtureCapabilities(overrides: Partial<RunnerCapabilities> = {}): RunnerCapabilities {
  return {
    protocol_version: 1,
    runner_version: '0.1.0',
    reported_at: '2026-08-06T12:00:00Z',
    labels: { os: 'linux', arch: 'x86_64', trust: 'local' },
    concurrency: { total: 2, available: 1 },
    harnesses: [
      {
        harness_kind: 'codex',
        installed_version: '1.2.3',
        probe_error: null,
        probed_at: '2026-08-06T11:59:55Z',
        model_selection: 'required',
      },
    ],
    features: {
      cancel: { support: 'supported', reason: null },
      resume: { support: 'unsupported', reason: 'installed harness has no resumable session contract' },
      decisions: { support: 'supported', reason: null },
      artifacts: { support: 'supported', reason: null },
      usage: { support: 'advisory', reason: 'token totals may be absent from harness output' },
    },
    limits: { event_payload_bytes_max: 65536, artifact_content_bytes_max: 52428800 },
    ...overrides,
  };
}

describe('gateFeature', () => {
  it('enables a supported feature and passes through a null reason unfabricated', () => {
    const gate = gateFeature(fixtureCapabilities(), 'cancel');
    expect(gate).toEqual({ enabled: true, reason: null });
  });

  it('disables an unsupported feature and keeps the runner-supplied reason', () => {
    const gate = gateFeature(fixtureCapabilities(), 'resume');
    expect(gate).toEqual({
      enabled: false,
      reason: 'installed harness has no resumable session contract',
    });
  });

  it('enables an advisory feature (advisory is not the same as unsupported)', () => {
    const gate = gateFeature(fixtureCapabilities(), 'usage');
    expect(gate.enabled).toBe(true);
  });
});

describe('gateFeatureAcrossRunners', () => {
  it('is disabled with an explicit reason when there is no capability data at all', () => {
    const gate = gateFeatureAcrossRunners([], 'cancel');
    expect(gate).toEqual({
      enabled: false,
      reason: 'no runner capability data available',
      supportingRunnerCount: 0,
      totalRunnerCount: 0,
    });
  });

  it('is enabled the moment at least one runner supports it, and counts supporters', () => {
    const supporting = fixtureCapabilities();
    const notSupporting = fixtureCapabilities({
      features: {
        ...fixtureCapabilities().features,
        cancel: { support: 'unsupported', reason: 'no cancellation hook' },
      },
    });
    const gate = gateFeatureAcrossRunners([notSupporting, supporting], 'cancel');
    expect(gate.enabled).toBe(true);
    expect(gate.supportingRunnerCount).toBe(1);
    expect(gate.totalRunnerCount).toBe(2);
  });

  it('is disabled only when every runner reports unsupported, and surfaces a reason', () => {
    const a = fixtureCapabilities({
      features: { ...fixtureCapabilities().features, cancel: { support: 'unsupported', reason: 'reason a' } },
    });
    const b = fixtureCapabilities({
      features: { ...fixtureCapabilities().features, cancel: { support: 'unsupported', reason: 'reason b' } },
    });
    const gate = gateFeatureAcrossRunners([a, b], 'cancel');
    expect(gate.enabled).toBe(false);
    expect(gate.supportingRunnerCount).toBe(0);
    expect(['reason a', 'reason b']).toContain(gate.reason);
  });
});

describe('listReportedHarnessKinds', () => {
  it('is empty for no snapshots', () => {
    expect(listReportedHarnessKinds([])).toEqual([]);
  });

  it('deduplicates and sorts across multiple runners', () => {
    const codex = fixtureCapabilities();
    const claude = fixtureCapabilities({
      harnesses: [
        {
          harness_kind: 'claude-code',
          installed_version: '0.5.0',
          probe_error: null,
          probed_at: '2026-08-06T12:00:00Z',
        },
      ],
    });
    expect(listReportedHarnessKinds([codex, claude, codex])).toEqual(['claude-code', 'codex']);
  });
});

describe('harnessProbeStatus', () => {
  it('reports probed:true when at least one runner reports a clean probe', () => {
    expect(harnessProbeStatus([fixtureCapabilities()], 'codex')).toEqual({ probed: true, lastError: null });
  });

  it('reports the last error when every report for the harness failed to probe', () => {
    const failed = fixtureCapabilities({
      harnesses: [
        {
          harness_kind: 'codex',
          installed_version: '1.2.3',
          probe_error: 'binary not found on PATH',
          probed_at: '2026-08-06T12:00:00Z',
        },
      ],
    });
    expect(harnessProbeStatus([failed], 'codex')).toEqual({
      probed: false,
      lastError: 'binary not found on PATH',
    });
  });

  it('a harness never reported at all is neither probed nor errored', () => {
    expect(harnessProbeStatus([fixtureCapabilities()], 'claude_code')).toEqual({
      probed: false,
      lastError: null,
    });
  });
});
