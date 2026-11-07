import type { FolderCheck } from '../api/projects';
import type { AgentSafetyReport } from '../help/texts';

/** The runner's own sentence for a profile that runs no commands, so the dialog and the timeline agree. */
export const NO_INSTALL = 'No install: this profile runs no commands';

export const INSTALL_OFF = 'Install turned off';
export const NO_LOCKFILE = 'No lockfile: nothing is installed';

/** Where the install of a run happens, in the words of the Prepares line. */
export const INSTALL_CONTAINMENT = {
  inside: 'Installs inside the sandbox',
  outside: 'Installs without a sandbox (the administrator turned the requirement off)',
} as const;

/**
 * Where the install runs, or null when the line has nothing to say: nothing is installed, docket
 * installs on its own, or the agent does not run on this computer. The install runs in the same
 * sandbox as the agent, so across several computers the least contained one decides.
 */
export function installContainment(line: string | null, reports: readonly AgentSafetyReport[]): string | null {
  if (line === null || line === NO_INSTALL || line === INSTALL_OFF || line === NO_LOCKFILE) return null;
  // Where the agent does not run (no sandbox and the requirement on), nothing is installed either.
  const runs = reports.filter((r) => r.harness_kind !== 'docket' && (r.sandbox === 'bwrap' || (r.sandbox === 'none' && r.sandbox_required === false)));
  if (runs.length === 0) return null;
  return runs.some((r) => r.sandbox === 'none') ? INSTALL_CONTAINMENT.outside : INSTALL_CONTAINMENT.inside;
}

/**
 * One sentence for what a run installs before the agent starts, or null when nothing is shown. `check`
 * is the folder check of the project's local folder; without one the board cannot read the repository,
 * so the runner decides.
 */
export function installLine(
  prepareCommand: string | null | undefined,
  check: FolderCheck | null | undefined,
  /** The chosen profile's `runs_commands`: a profile that runs none installs nothing. */
  runsCommands?: boolean,
  /** The chosen profile's team works in a folder: it has no copy to install into, so no line is shown. */
  worksInFolder?: boolean,
): string | null {
  if (worksInFolder) return null;
  if (runsCommands === false) return NO_INSTALL;
  if (prepareCommand === '') return INSTALL_OFF;
  if (prepareCommand) return `Replaced: ${prepareCommand}`;
  if (!check?.is_git) return 'Detected on the runner when the run starts';
  return check.install
    ? `Prepares with ${check.install.command} (from ${check.install.lockfile})`
    : NO_LOCKFILE;
}
