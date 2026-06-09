import { request } from './client';
import type { Project, CreateProject, UpdateProject } from '../types';

export const projects = {
  list: () => request<Project[]>('/projects'),

  get: (id: string) => request<Project>(`/projects/${id}`),

  create: (data: CreateProject) =>
    request<{ id: string }>('/projects', {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  update: (id: string, data: UpdateProject) =>
    request<Project>(`/projects/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    }),

  remove: (id: string) =>
    request<void>(`/projects/${id}`, { method: 'DELETE' }),
};
