import type { FolderCheck } from '../api/projects';

/**
 * One sentence for what a run installs before the agent starts. `check` is the folder check of the
 * project's local folder; without one the board cannot read the repository, so the runner decides.
 */
export function installLine(prepareCommand: string | null | undefined, check: FolderCheck | null | undefined): string {
  if (prepareCommand === '') return 'Install turned off';
  if (prepareCommand) return `Replaced: ${prepareCommand}`;
  if (!check?.is_git) return 'Detected on the runner when the run starts';
  return check.install
    ? `Prepares with ${check.install.command} (from ${check.install.lockfile})`
    : 'No lockfile: nothing is installed';
}
