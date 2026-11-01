import type { Project } from '../types';

/**
 * Why a project's Change sets page is off, or null when it is on. It is on when the project lets an
 * agent propose (any rung above Read) or when a change set waits in it; the page reads the rest.
 */
export function changeSetsOffReason(project: Pick<Project, 'agents_on_board'> | undefined, waiting: number): string | null {
  if (!project || waiting > 0) return null;
  if (project.agents_on_board?.level === 'read') {
    return 'Agents may only read in this project, so none proposes changes. Raise the rung Agents on the board in Settings, Automation.';
  }
  return null;
}
