import { request } from './client';
import type { components } from './schema.gen';

type Schemas = components['schemas'];

/** A list of operations on one project's board, from one principal. */
export type ChangeSet = Schemas['ChangeSet'];
export type ChangeOp = Schemas['ChangeOp'];
export type ChangeSetStatus = Schemas['ChangeSetStatus'];
/** One change to the board: a create, who does it, a priority, a move, a dependency, a comment or a due date. */
export type Operation = Schemas['Operation'];
export type ItemRef = Schemas['ItemRef'];
export type AgentsOnBoard = Schemas['AgentsOnBoard'];
export type RungLevel = Schemas['RungLevel'];

export const changeSets = {
  /** A project's change sets, the newest first; `status` limits them to those states. */
  list: (projectId: string, status?: ChangeSetStatus[], limit?: number) => {
    const query = new URLSearchParams();
    if (status && status.length > 0) query.set('status', status.join(','));
    if (limit) query.set('limit', String(limit));
    const text = query.toString();
    return request<ChangeSet[]>(`/projects/${projectId}/change-sets${text ? `?${text}` : ''}`);
  },
  get: (id: string) => request<ChangeSet>(`/change-sets/${id}`),
  /** Applies the chosen operations, or everything that waits when `operations` is absent. */
  apply: (id: string, operations?: number[]) =>
    request<ChangeSet>(`/change-sets/${id}/apply`, {
      method: 'POST',
      body: JSON.stringify(operations ? { operations } : {}),
    }),
  /** Puts `operation` in place of the waiting one at `index`; it is checked when applied. */
  edit: (id: string, index: number, operation: Operation) =>
    request<ChangeSet>(`/change-sets/${id}/operations/${index}`, {
      method: 'PATCH',
      body: JSON.stringify({ operation }),
    }),
  reject: (id: string, reason: string) =>
    request<ChangeSet>(`/change-sets/${id}/reject`, { method: 'POST', body: JSON.stringify({ reason }) }),
};

/**
 * The reasons a refused change set lists, one per line. The server writes "The change set cannot be
 * applied: <reason>" for one and "..., N reasons:" followed by a line each for several; any other
 * message is the one reason.
 */
export function refusalReasons(message: string): string[] {
  const lines = message.split('\n').map((l) => l.trim()).filter((l) => l !== '');
  if (lines.length > 1 && /^The change set cannot be applied, \d+ reasons:?$/.test(lines[0])) return lines.slice(1);
  const single = /^The change set cannot be applied: (.+)$/.exec(lines[0] ?? '');
  if (lines.length === 1 && single) return [single[1]];
  return lines;
}
