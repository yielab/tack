import { request } from './client';
import type { Role, CreateRole } from '../types';
import type { components } from './schema.gen';

export const roles = {
  /** Roles/specialties defined for a project. */
  list: (projectId: string) =>
    request<Role[]>(`/projects/${projectId}/roles`),

  create: (projectId: string, data: CreateRole) =>
    request<Role>(`/projects/${projectId}/roles`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  /** `PATCH /roles/{id}` — rename, recolor, or set the profile a task with this role runs with. */
  update: (roleId: string, data: components['schemas']['UpdateRole']) =>
    request<Role>(`/roles/${roleId}`, { method: 'PATCH', body: JSON.stringify(data) }),

  remove: (roleId: string) =>
    request<void>(`/roles/${roleId}`, { method: 'DELETE' }),

  /** Assign a role to an item. */
  assign: (itemId: string, roleId: string) =>
    request<void>(`/items/${itemId}/roles/${roleId}`, { method: 'PUT' }),

  /** Remove a role assignment from an item. */
  unassign: (itemId: string, roleId: string) =>
    request<void>(`/items/${itemId}/roles/${roleId}`, { method: 'DELETE' }),
};
