import { request } from './client';
import type { Project, CreateProject, UpdateProject } from '../types';

export interface FolderCheck {
  exists: boolean;
  is_dir: boolean;
  is_git: boolean;
  branch: string | null;
  remote_url: string | null;
  dirty_files: number;
}

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

  /** `POST /local-runner/check-folder` — what is at `path`; writes nothing. */
  checkFolder: (path: string) =>
    request<FolderCheck>('/local-runner/check-folder', {
      method: 'POST',
      body: JSON.stringify({ path }),
    }),

  /** `POST /local-runner/init-folder` — creates `path` and runs `git init`. */
  initFolder: (path: string) =>
    request<void>('/local-runner/init-folder', {
      method: 'POST',
      body: JSON.stringify({ path }),
    }),

  remove: (id: string) =>
    request<void>(`/projects/${id}`, { method: 'DELETE' }),
};
