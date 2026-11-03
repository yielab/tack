// The teams any agent reports (`GET /api/teams`), as Tack describes them: every field is optional
// and absent means the agent did not say, which is never the same as "none".

import type { components } from '../api/schema.gen';
import { request } from '../api/client';

export type TeamEntry = components['schemas']['TeamEntry'];
export type TeamReport = components['schemas']['TeamReport'];
export type TeamStep = components['schemas']['TeamStep'];
export type TeamMember = components['schemas']['TeamMember'];
export type TeamList = components['schemas']['TeamList'];
export type TeamMisfit = components['schemas']['TeamMisfit'];

export const teamsApi = {
  /** Every team any agent reports, then the names only profiles use. */
  list: async (): Promise<TeamEntry[]> => (await request<Partial<TeamList>>('/teams')).teams ?? [],
};

/** The names a Team field offers: the reported teams first, then names only profiles use. */
export function teamChoices(
  entries: readonly TeamEntry[],
  alsoNamed: readonly (string | null | undefined)[] = [],
): { reported: string[]; notReported: string[] } {
  const reported = new Set(entries.filter((e) => e.reported).map((e) => e.name));
  const notReported = new Set(entries.filter((e) => !e.reported).map((e) => e.name));
  for (const name of alsoNamed) if (name && !reported.has(name)) notReported.add(name);
  return {
    reported: [...reported].sort(),
    notReported: [...notReported].filter((n) => !reported.has(n)).sort(),
  };
}

const CHECKED_BY: Record<NonNullable<TeamStep['checked_by']>, string> = {
  command: 'a command',
  reviewer: 'a reviewer',
  person: 'a person',
  external: 'an external check',
};

const STEP_KIND: Record<TeamStep['kind'], string> = {
  agent: '',
  command: 'runs a command',
  ask: 'asks a person',
  wait: 'waits',
  parallel: 'runs side by side',
};

/** One step in words: "write · Lead · checked by a reviewer · sent back to write up to 2 times". */
export function stepText(step: TeamStep): string {
  const parts: string[] = [step.name];
  // A step named after its role ("lead" run by Lead) says it once.
  if (step.role) {
    if (step.role.toLowerCase() !== step.name.toLowerCase()) parts.push(step.role);
  } else if (STEP_KIND[step.kind]) parts.push(STEP_KIND[step.kind]);
  if (step.checked_by) parts.push(`checked by ${CHECKED_BY[step.checked_by]}`);
  if (step.sends_back_to) {
    const max = step.sends_back_max;
    parts.push(
      max == null ? `sent back to ${step.sends_back_to}` : `sent back to ${step.sends_back_to} up to ${max} ${max === 1 ? 'time' : 'times'}`,
    );
  }
  return parts.join(' · ');
}

const NETWORK_TEXT = { none: 'No network', limited: 'Limited network', open: 'Open network' } as const;

export function networkText(network: NonNullable<TeamReport['network']>): string {
  return NETWORK_TEXT[network];
}

export function queueText(queue: { waiting: number; running: number }): string {
  return `${queue.waiting} waiting, ${queue.running} running`;
}

/** What a member can do, in plain words, for whatever the agent reported of it: nothing is said of a
 *  fact it left out. "Reads only" and "Changes the project" are the two answers to the same question;
 *  what it reads of the web and "Can propose items" are said only when true, the latter always last. */
export function memberReachText(member: TeamMember): string[] {
  const words: string[] = [];
  if (member.writes_project === true) words.push('Changes the project');
  else if (member.writes_project === false) words.push('Reads only');
  const web = webText(member);
  if (web) words.push(web);
  if (member.can_propose === true) words.push('Can propose items');
  return words;
}

/** What a member reads of the web, said only when the agent reported that it does: the hosts when
 *  it named them, `*` read as any public host. */
export function webText(member: TeamMember): string | null {
  if (member.reads_web !== true) return null;
  const hosts = member.web_hosts ?? [];
  if (hosts.length === 0) return 'Reads the web';
  return hosts.includes('*') ? 'Reads the web: any public host' : `Reads the web: ${hosts.join(', ')}`;
}

/** Why a profile's runs on `team` are refused before the team gets them, and the change in docket
 *  that ends it. Follows the profile's name. */
export function misfitText(team: string, misfit: TeamMisfit): string {
  return misfit === 'no_web'
    ? `needs the web, and this team has no network and no member that reads the web. Give it the hosts it reads with \`docket pod set fetchDomains <host> --pod ${team}\`, open its whole network with \`docket pod set network open --widen --pod ${team}\`, or turn the network off in the profile.`
    : `delivers only proposed items, and no member of this team can propose. Declare the team's \`tack_proposals\` record set and let a role write it.`;
}

/** The one sentence a team whose proposals are kept for all its tasks together is owed. */
export const SHARED_PROPOSALS_TEXT =
  "This team's proposals are kept for all its tasks together: a name used in one task cannot be used again in another until the team's proposal list is recreated.";

/** What a paused team is owed: the fact in a few words and the one way to end it. Tack has no
 *  control to resume a team (the agent has no route for it), so the sentence is the reason. */
export function pausedText(name: string, paused: NonNullable<TeamReport['paused']>): { headline: string; resume: string } {
  return {
    headline: paused === 'budget' ? 'Paused: budget spent' : 'Paused',
    resume: `Resume it with \`docket run --pod ${name} --resume\`; its queued tasks wait until then.`,
  };
}

/** What the team's report says of proposing, read from the one field both the card and Compare use:
 *  each member's `can_propose`. `roles` are the members that can; `not_reported` when the report does not say. */
export function teamProposes(report: TeamReport | null | undefined): { state: 'can' | 'cannot' | 'not_reported'; roles: string[] } {
  const members = report?.members;
  if (!members || members.length === 0) return { state: 'not_reported', roles: [] };
  const roles = members.filter((m) => m.can_propose === true).map((m) => m.role);
  if (roles.length > 0) return { state: 'can', roles };
  return members.every((m) => m.can_propose === false) ? { state: 'cannot', roles: [] } : { state: 'not_reported', roles: [] };
}

/** A name the agent reported as it is written, without the backticks a sentence may wrap it in. */
export const plainName = (text: string): string => text.replaceAll('`', '');
