import { request } from './client';
import type { components } from './schema.gen';

type Schemas = components['schemas'];

/** A rule that wakes an agent profile on an event or at a time. */
export type Trigger = Schemas['Trigger'];
export type TriggerSource = Schemas['Source'];
export type TriggerEvent = Schemas['EventKind'];
export type TriggerFilter = Schemas['Filter'];
export type TriggerSchedule = Schemas['Schedule'];
export type CreateTrigger = Schemas['CreateTrigger'];
export type UpdateTrigger = Schemas['UpdateTrigger'];

export const triggers = {
  /** The triggers, oldest first; `projectId` limits them to one project's. */
  list: (projectId?: string) =>
    request<Trigger[]>(`/triggers${projectId ? `?project_id=${encodeURIComponent(projectId)}` : ''}`),
  create: (input: CreateTrigger) => request<Trigger>('/triggers', { method: 'POST', body: JSON.stringify(input) }),
  update: (id: string, patch: UpdateTrigger) =>
    request<Trigger>(`/triggers/${encodeURIComponent(id)}`, { method: 'PATCH', body: JSON.stringify(patch) }),
  remove: (id: string) => request<void>(`/triggers/${encodeURIComponent(id)}`, { method: 'DELETE' }),
};
