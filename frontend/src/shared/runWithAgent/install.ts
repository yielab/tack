import type { FolderCheck } from '../api/projects';

/** The runner's own sentence for a profile that runs no commands, so the dialog and the timeline agree. */
export const NO_INSTALL = 'No install: this profile runs no commands';

/**
 * One sentence for what a run installs before the agent starts. `check` is the folder check of the
 * project's local folder; without one the board cannot read the repository, so the runner decides.
 */
export function installLine(
  prepareCommand: string | null | undefined,
  check: FolderCheck | null | undefined,
  /** The chosen profile's `runs_commands`: a profile that runs none installs nothing. */
  runsCommands?: boolean,
): string {
  if (runsCommands === false) return NO_INSTALL;
  if (prepareCommand === '') return 'Install turned off';
  if (prepareCommand) return `Replaced: ${prepareCommand}`;
  if (!check?.is_git) return 'Detected on the runner when the run starts';
  return check.install
    ? `Prepares with ${check.install.command} (from ${check.install.lockfile})`
    : 'No lockfile: nothing is installed';
}
