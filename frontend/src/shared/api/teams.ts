import { request } from './client';
import type { components } from './schema.gen';

type Schemas = components['schemas'];

/**
 * A team to make from the repository's `.docket/teams/<name>` folder: a row of the import preview's
 * `missing` list. `accept` is the code of an earlier `needs_review` for this team, once the person read the commands.
 */
export type TeamToApply = Schemas['TeamToApply'];
/** How one team went: `applied`, `needs_review` (read `review`, then send the team again with its code) or `refused` (read `message`). */
export type TeamApplyState = Schemas['TeamApplyState'];
/** What docket asks a person to read before it applies the folder: a code and what it would run. */
export type TeamApplyReview = Schemas['TeamApplyReview'];
export type TeamApplyResult = Schemas['TeamApplyResult'];

/**
 * `POST /projects/{id}/teams/apply`: asks a computer that runs docket to make each team from its shape and apply
 * the project's folder for it, and answers once each has gone (up to 30 seconds a team). Refused with `409` (the
 * sentence says what to do) unless an active runner reports a docket that lists `podShape`; the Teams screen enables
 * its button with `gateApiFeature(snapshots, 'docket', 'podShape')`, not by calling this.
 */
export const applyTeams = (projectId: string, teams: TeamToApply[]) =>
  request<TeamApplyResult[]>(`/projects/${projectId}/teams/apply`, { method: 'POST', body: JSON.stringify({ teams }) });

export const teams = { applyTeams };
