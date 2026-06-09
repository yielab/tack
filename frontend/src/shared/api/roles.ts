import { request } from './client';
import type { Role, CreateRole } from '../types';

export const roles = {
  /** Roles/specialties defined for a project. */
  list: (projectId: string) =>
    request<Role[]>(`/projects/${projectId}/roles`),

  create: (projectId: string, data: CreateRole) =>
    request<Role>(`/projects/${projectId}/roles`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  remove: (roleId: string) =>
    request<void>(`/roles/${roleId}`, { method: 'DELETE' }),

  /** Assign a role to an item. */
  assign: (itemId: string, roleId: string) =>
    request<void>(`/items/${itemId}/roles/${roleId}`, { method: 'PUT' }),

  /** Remove a role assignment from an item. */
  unassign: (itemId: string, roleId: string) =>
    request<void>(`/items/${itemId}/roles/${roleId}`, { method: 'DELETE' }),
};
