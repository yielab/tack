// Pure capability-selector logic: what harnesses and features
// are actually usable, driven by real runner-reported capability data, not
// assumptions.
//
// Every function here takes `RunnerCapabilities[]` (see `types.ts`) as a
// plain argument — it performs no I/O and knows nothing about HTTP, caching,
// or where the snapshots came from, the same "pure selection, never grants
// the authoritative lease" split `crates/tack-orch/src/scheduler` draws
// server-side, applied to the frontend's read-side equivalent: a component
// decides whether to let an operator pick a harness,
// never whether to actually grant a lease.
//
// `GET /runners` feeds these functions today via `RunWithAgentModal.tsx`'s
// `runnerSummaryToCapabilities` adapter (see `types.ts`'s `RunnerCapabilities`
// doc comment) — every function below is tested against the frozen
// `docs/contracts/runner-v1/capabilities.json` fixture shape.
//
// No function in this file ever reports `supported: true` without at least
// one runner's real capability data backing it, and never reports
// `supported: false` without a specific, typed reason — unsupported is
// typed, unknown is explicit, extended to this module's own vocabulary: an
// empty snapshot list is `false` with the explicit reason `'no runner
// capability data available'`, never a structural zero indistinguishable
// from "checked, and it's unsupported."

import type { CapabilitySupport, CapabilityValue, FeatureCapabilities, RunnerCapabilities } from './types';

/** What a gated control needs to render itself. `reason` stays nullable
 *  because `CapabilityValue.reason` is genuinely `null` on the wire when a
 *  runner reports support without qualification
 *  (`docs/contracts/runner-v1/capabilities.json`'s `cancel` entry) — a
 *  component must not fabricate text where the runner gave none. */
export interface CapabilityGate {
  enabled: boolean;
  reason: string | null;
}

const NO_DATA_REASON = 'no runner capability data available';

/** A single runner capability value is "enabled" at every level except
 *  `'unsupported'` (`'advisory'` still lets a control fire). */
function gateValue(value: CapabilityValue | undefined): CapabilityGate {
  if (!value) return { enabled: false, reason: NO_DATA_REASON };
  return { enabled: value.support !== 'unsupported', reason: value.reason };
}

export type FeatureName = keyof FeatureCapabilities;

/** Gates one feature (`cancel`/`resume`/`decisions`/`artifacts`/`usage`) for
 *  a single runner's capability report. */
export function gateFeature(capabilities: RunnerCapabilities, feature: FeatureName): CapabilityGate {
  return gateValue(capabilities.features[feature]);
}

/**
 * Gates one feature across every runner that could serve a fleet/`any`
 * selector. `enabled` is true the moment at least one runner supports it
 * (advisory or supported) — the scheduler, not this function, decides
 * which runner within the fleet actually gets picked; this is only
 * "could the operator ever expect this control to do something." Reports
 * `supportingRunnerCount`/`totalRunnerCount` so a caller can render
 * "supported by 2 of 3 runners" rather than a bare boolean.
 */
export function gateFeatureAcrossRunners(
  snapshots: RunnerCapabilities[],
  feature: FeatureName,
): CapabilityGate & { supportingRunnerCount: number; totalRunnerCount: number } {
  if (snapshots.length === 0) {
    return { enabled: false, reason: NO_DATA_REASON, supportingRunnerCount: 0, totalRunnerCount: 0 };
  }
  const gates = snapshots.map((snapshot) => gateValue(snapshot.features[feature]));
  const supporting = gates.filter((gate) => gate.enabled);
  const supportingRunnerCount = supporting.length;
  const totalRunnerCount = snapshots.length;
  if (supportingRunnerCount > 0) {
    const reason = supporting.find((gate) => gate.reason !== null)?.reason ?? null;
    return { enabled: true, reason, supportingRunnerCount, totalRunnerCount };
  }
  const reason = gates.find((gate) => gate.reason !== null)?.reason ?? null;
  return { enabled: false, reason, supportingRunnerCount: 0, totalRunnerCount };
}

/** Every distinct `harness_kind` reported by at least one runner, sorted for
 *  a stable render order. A harness whose only report carries a
 *  `probe_error` is still listed — the operator should see it exists, even
 *  disabled; use {@link harnessProbeStatus} to gate the picker entry. */
export function listReportedHarnessKinds(snapshots: RunnerCapabilities[]): string[] {
  const kinds = new Set<string>();
  for (const snapshot of snapshots) {
    for (const harness of snapshot.harnesses) kinds.add(harness.harness_kind);
  }
  return [...kinds].sort();
}

/** Whether at least one runner reports this harness with no `probe_error`,
 *  and the most recent probe error seen for it otherwise (never silently
 *  hidden — a component must be able to say why a harness is greyed out). */
export function harnessProbeStatus(
  snapshots: RunnerCapabilities[],
  harnessKind: string,
): { probed: boolean; lastError: string | null } {
  let lastError: string | null = null;
  let sawClean = false;
  for (const snapshot of snapshots) {
    for (const harness of snapshot.harnesses) {
      if (harness.harness_kind !== harnessKind) continue;
      if (harness.probe_error === null) sawClean = true;
      else lastError = harness.probe_error;
    }
  }
  return { probed: sawClean, lastError: sawClean ? null : lastError };
}

export type { CapabilitySupport };
