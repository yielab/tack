import { request } from './client';
import type { components } from './schema.gen';

type Schemas = components['schemas'];

/** One area of the code Tack may merge in by itself. `merged` and `undone` are read only. */
export type AutonomyDomain = Schemas['DomainSettings'] & {
  merged?: number;
  undone?: number;
};

/**
 * `GET`/`PUT /projects/{id}/autonomy`. `automatic_merge_blocked_by` and the
 * domains' counts are read only (the server ignores them on a PUT); the
 * generated schema predates them, so they are declared here.
 */
export type AutonomySettings = Omit<Schemas['AutonomySettings'], 'domains'> & {
  domains: AutonomyDomain[];
  /** Sentences saying why Tack cannot merge by itself on this board; empty when nothing stops it. */
  automatic_merge_blocked_by?: string[];
};

export const autonomy = {
  get: (projectId: string) => request<AutonomySettings>(`/projects/${projectId}/autonomy`),
  put: (projectId: string, settings: AutonomySettings) =>
    request<AutonomySettings>(`/projects/${projectId}/autonomy`, { method: 'PUT', body: JSON.stringify(settings) }),
};
