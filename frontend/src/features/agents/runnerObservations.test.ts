import { describe, it, expect } from 'vitest';
import {
  countOtherActiveRunners,
  findThisMachineRunner,
  harnessesOf,
} from './runnerObservations';
import type { RunnerSummary } from '../../shared/execution/api';

function runner(overrides: Partial<RunnerSummary>): RunnerSummary {
  return {
    runner_id: 'runr_1',
    name: 'runner-1',
    state: 'active',
    labels: null,
    labels_raw: '{}',
    total_capacity: 1,
    available_capacity: 1,
    capability_snapshot: null,
    capability_snapshot_raw: '{}',
    protocol_version: 1,
    runner_version: '0.1.0',
    last_heartbeat_at: null,
    revoked_at: null,
    fleet_ids: [],
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

describe('findThisMachineRunner / countOtherActiveRunners', () => {
  it('finds only an active runner named with the local- prefix', () => {
    const runners = [
      runner({ runner_id: 'a', name: 'local-abc', state: 'active' }),
      runner({ runner_id: 'b', name: 'remote-fleet-box', state: 'active' }),
    ];
    expect(findThisMachineRunner(runners)?.runner_id).toBe('a');
    expect(countOtherActiveRunners(runners)).toBe(1);
  });

  it('never counts a revoked or pending local- row as this machine', () => {
    const runners = [runner({ name: 'local-abc', state: 'revoked' })];
    expect(findThisMachineRunner(runners)).toBeNull();
    expect(countOtherActiveRunners(runners)).toBe(0);
  });

  it('does not count this machine\'s own runner as an "other machine"', () => {
    const runners = [runner({ name: 'local-abc', state: 'active' })];
    expect(countOtherActiveRunners(runners)).toBe(0);
  });
});

describe('harnessesOf', () => {
  it('returns [] for a null or malformed capability_snapshot rather than throwing', () => {
    expect(harnessesOf(null)).toEqual([]);
    expect(harnessesOf(runner({ capability_snapshot: { not_harnesses: true } }))).toEqual([]);
    expect(harnessesOf(runner({ capability_snapshot: { harnesses: 'not-an-array' } }))).toEqual([]);
  });

  it('returns the real array when the snapshot is well-formed', () => {
    const snap = { harnesses: [{ harness_kind: 'codex', installed_version: '1.0.0', probe_error: null, probed_at: '' }] };
    expect(harnessesOf(runner({ capability_snapshot: snap }))).toHaveLength(1);
  });
});
