// Which agent profiles may run an item (`GET /api/items/{id}/run-options`): the server's rule, read
// here and never worked out again. A group is planned: only a profile that plans or researches runs it.

import type { components } from '../api/schema.gen';
import { request } from '../api/client';

export type RunOption = components['schemas']['RunOption'];

/** What the Run dialog knows of an item before anyone picks a profile. */
export interface RunChoices {
  /** The item holds work: its type does, or items sit under it. */
  group: boolean;
  /** Why a profile may not run it, by profile id; a profile that may is absent. */
  refused: ReadonlyMap<string, string>;
}

export const NO_CHOICES: RunChoices = { group: false, refused: new Map() };

function choicesOf(options: unknown): RunChoices {
  if (!Array.isArray(options)) return NO_CHOICES;
  const rows = options as RunOption[];
  return {
    group: rows.some((o) => o.node === 'group'),
    refused: new Map(rows.filter((o) => !o.allowed && o.reason).map((o) => [o.profile_id, o.reason as string])),
  };
}

export const runOptionsApi = {
  get: (itemId: string): Promise<RunChoices> => request<RunOption[]>(`/items/${itemId}/run-options`).then(choicesOf),
};
