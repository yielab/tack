// The teams any agent reports (`GET /api/teams`), as Tack describes them: every field is optional
// and absent means the agent did not say, which is never the same as "none".

import type { components } from '../api/schema.gen';
import { request } from '../api/client';

export type TeamEntry = components['schemas']['TeamEntry'];
export type TeamReport = components['schemas']['TeamReport'];
export type TeamStep = components['schemas']['TeamStep'];
export type TeamList = components['schemas']['TeamList'];

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
  if (step.role) parts.push(step.role);
  else if (STEP_KIND[step.kind]) parts.push(STEP_KIND[step.kind]);
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
