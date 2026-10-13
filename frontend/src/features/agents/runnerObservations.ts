// Pure helpers over `GET /api/runners` (`shared/execution/api.ts`'s
// `RunnerSummary`) for the Agents page's steps 1, 2 and 4 — kept separate
// from any component so "which runner is this machine's own" and "what do
// active runners collectively support" are each one small, testable
// function instead of logic buried inside JSX.
//
// Live-observed convention this module depends on: the embedded/
// self-provisioned runner (`docs/CONFIG.md`'s "Embedded runner" section,
// ADR 0061 decision 6) is always named `local-<uuid>` — confirmed against a
// real `tack serve --with-runner` process across a restart (the name is
// stable, reused from the on-disk credential, never regenerated). There is
// no dedicated wire field for "is this the embedded runner"; this is the
// only observable signal, so `isThisMachineRunner` names it as a convention
// rather than a guaranteed contract — a future runner-v1 revision that adds
// an explicit field should replace this, not stack on top of it.

import type { RunnerSummary } from '../../shared/execution/api';
import type { HarnessCapability, RunnerCapabilities } from '../../shared/execution/types';

export const LOCAL_RUNNER_NAME_PREFIX = 'local-';

export function isThisMachineRunner(runner: RunnerSummary): boolean {
  return runner.name.startsWith(LOCAL_RUNNER_NAME_PREFIX);
}

/** The active runner representing the embedded runner on this machine, or
 *  `null` when none is enrolled (never started, or revoked). Never used to
 *  claim "on" by itself — step 1's own state comes from `GET
 *  /api/local-runner`; this is only for step 2's per-harness listing, which
 *  has no other source. */
export function findThisMachineRunner(runners: readonly RunnerSummary[]): RunnerSummary | null {
  return runners.find((r) => r.state === 'active' && isThisMachineRunner(r)) ?? null;
}

/** Active runners that are not this machine's own — "on, other machines" in
 *  step 1. Counts every `state === 'active'` row regardless of how recent
 *  its heartbeat is: staleness has no agreed threshold anywhere else in
 *  this tree, and inventing one here would be a hard-coded status this
 *  codebase's rules forbid. A caller that wants
 *  to show staleness can read `last_heartbeat_at` directly. */
export function countOtherActiveRunners(runners: readonly RunnerSummary[]): number {
  return runners.filter((r) => r.state === 'active' && !isThisMachineRunner(r)).length;
}

/** `capability_snapshot` is an untyped, best-effort parse of a stored JSON
 *  column server-side (`RunnerSummary`'s own doc comment) — never assumed
 *  to have the expected shape. Returns `[]` rather than throwing when it
 *  doesn't parse as a `RunnerCapabilities`-shaped object with a `harnesses`
 *  array. */
export function harnessesOf(runner: RunnerSummary | null): HarnessCapability[] {
  const snapshot = runner?.capability_snapshot as Partial<RunnerCapabilities> | null | undefined;
  const harnesses = snapshot?.harnesses;
  return Array.isArray(harnesses) ? harnesses : [];
}

export function findHarness(
  harnesses: readonly HarnessCapability[],
  harnessKind: string,
): HarnessCapability | undefined {
  return harnesses.find((h) => h.harness_kind === harnessKind);
}
