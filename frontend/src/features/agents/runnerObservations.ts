// Pure helpers over `GET /api/runners` (`shared/execution/api.ts`'s
// `RunnerSummary`) for the Agents page's steps 1, 2 and 4 — kept separate
// from any component so "which runner is this machine's own" and "what do
// active runners collectively support" are each one small, testable
// function instead of logic buried inside JSX.
//
// Live-observed convention this module depends on: the embedded/
// self-provisioned runner (`docs/CONFIG.md`'s "Embedded runner" section,
// ADR 0005 decision 6) is named `local-<uuid>`, or "This computer (host name)" once the server renamed it — confirmed against a
// real `tack serve --with-runner` process across a restart (the name is
// stable, reused from the on-disk credential, never regenerated). There is
// no dedicated wire field for "is this the embedded runner"; this is the
// only observable signal, so `isThisMachineRunner` names it as a convention
// rather than a guaranteed contract — a future runner-v1 revision that adds
// an explicit field should replace this, not stack on top of it.

import type { RunnerSummary } from '../../shared/execution/api';
import type { HarnessCapability, RunnerCapabilities } from '../../shared/execution/types';

export const LOCAL_RUNNER_NAME_PREFIX = 'local-';
/** What the embedded runner is named now: "This computer (host name)". A row enrolled before that
 *  keeps its generated `local-` name until the server renames it. */
export const THIS_COMPUTER_NAME_PREFIX = 'This computer';

export function isThisMachineRunner(runner: RunnerSummary): boolean {
  return runner.name.startsWith(LOCAL_RUNNER_NAME_PREFIX) || runner.name.startsWith(THIS_COMPUTER_NAME_PREFIX);
}

/** The active runner representing the embedded runner on this machine, or
 *  `null` when none is enrolled (never started, or revoked). A restart can
 *  leave several `local-*` rows: the online one with the newest heartbeat
 *  wins, else the newest. Never used to claim "on" by itself — step 1's own
 *  state comes from `GET /api/local-runner`; this is only for step 2's
 *  per-harness listing, which has no other source. */
export function findThisMachineRunner(runners: readonly RunnerSummary[]): RunnerSummary | null {
  const mine = runners.filter((r) => r.state === 'active' && isThisMachineRunner(r));
  const online = mine.filter((r) => r.online);
  const heartbeat = (r: RunnerSummary) => Date.parse(r.last_heartbeat_at ?? '') || 0;
  return [...(online.length > 0 ? online : mine)].sort((a, b) => heartbeat(b) - heartbeat(a))[0] ?? null;
}

/** Online runners that are not this machine's own — "on, other machines" in
 *  step 1. Whether a runner is online is the server's answer (`online`, from
 *  the scheduler's own rule); nothing here decides it. */
export function countOtherActiveRunners(runners: readonly RunnerSummary[]): number {
  return runners.filter((r) => r.online && !isThisMachineRunner(r)).length;
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

/** The probe found the agent: a version, or for an agent that is a service
 *  (docket, which publishes no version of its own) the API it speaks. */
export function isInstalled(h: HarnessCapability | undefined): h is HarnessCapability {
  return !!h && !!(h.installed_version || h.api_version);
}

/** Installed, and its probe found nothing wrong: the agent can be used here. */
export function isReady(h: HarnessCapability | undefined): h is HarnessCapability & { probe_error: null } {
  return isInstalled(h) && h.probe_error === null;
}

/** "v1.2.3", "API 5", or both; the "v" goes only before a version. */
export function versionText(h: HarnessCapability): string {
  return [h.installed_version && `v${h.installed_version}`, h.api_version && `API ${h.api_version}`]
    .filter(Boolean)
    .join(' · ');
}

export function findHarness(
  harnesses: readonly HarnessCapability[],
  harnessKind: string,
): HarnessCapability | undefined {
  return harnesses.find((h) => h.harness_kind === harnessKind);
}
