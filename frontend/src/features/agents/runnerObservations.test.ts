import { describe, it, expect } from 'vitest';
import {
  countOtherActiveRunners,
  findThisMachineRunner,
  harnessesOf,
  isInstalled,
  isReady,
  versionText,
} from './runnerObservations';
import type { RunnerSummary } from '../../shared/execution/api';
import type { HarnessCapability } from '../../shared/execution/types';

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
    online: true,
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

  it('finds the embedded runner under the name "This computer (host name)" too', () => {
    const runners = [
      runner({ runner_id: 'a', name: 'This computer (box)', state: 'active' }),
      runner({ runner_id: 'b', name: 'other-box', state: 'active' }),
    ];
    expect(findThisMachineRunner(runners)?.runner_id).toBe('a');
    expect(countOtherActiveRunners(runners)).toBe(1);
  });

  it('takes the online local- runner with the newest heartbeat, else the newest', () => {
    const rows = [
      runner({ runner_id: 'old', name: 'local-old', last_heartbeat_at: '2026-10-17T08:00:00Z', online: false }),
      runner({ runner_id: 'newer-offline', name: 'local-offline', last_heartbeat_at: '2026-10-17T11:00:00Z', online: false }),
      runner({ runner_id: 'online', name: 'local-online', last_heartbeat_at: '2026-10-17T10:00:00Z', online: true }),
    ];
    expect(findThisMachineRunner(rows)?.runner_id).toBe('online');
    expect(findThisMachineRunner(rows.slice(0, 2))?.runner_id).toBe('newer-offline');
  });

  it('counts only online runners as other machines', () => {
    const rows = [
      runner({ runner_id: 'a', name: 'box-a', online: true }),
      runner({ runner_id: 'b', name: 'box-b', online: false }),
    ];
    expect(countOtherActiveRunners(rows)).toBe(1);
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

describe('isInstalled, isReady and versionText', () => {
  const h = (o: Partial<HarnessCapability>) =>
    ({ harness_kind: 'x', installed_version: '', probe_error: null, probed_at: '', ...o }) as HarnessCapability;
  it.each([
    ['a binary with a version', h({ installed_version: '1.2.3' }), true, true, 'v1.2.3'],
    ['docket: no version of its own, only the API it speaks', h({ api_version: '5' }), true, true, 'API 5'],
    ['both', h({ installed_version: '0.2.0', api_version: '5' }), true, true, 'v0.2.0 · API 5'],
    ['installed but its probe failed', h({ installed_version: '1.0.0', probe_error: 'boom' }), true, false, 'v1.0.0'],
    ['not found', h({ probe_error: 'not found on PATH' }), false, false, ''],
  ])('%s', (_name, harness, installed, ready, text) => {
    expect(isInstalled(harness)).toBe(installed);
    expect(isReady(harness)).toBe(ready);
    expect(versionText(harness)).toBe(text);
  });
});
