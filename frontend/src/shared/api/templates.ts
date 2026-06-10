import { request } from './client';
import type {
  ProjectTemplate,
  CreateTemplate,
  CreateProjectFromTemplate,
} from '../types';

export const templates = {
  list: (projectType?: string) => {
    const qs = projectType
      ? `?${new URLSearchParams({ project_type: projectType })}`
      : '';
    return request<ProjectTemplate[]>(`/templates${qs}`);
  },

  get: (id: string) => request<ProjectTemplate>(`/templates/${id}`),

  create: (data: CreateTemplate) =>
    request<ProjectTemplate>('/templates', {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  remove: (id: string) =>
    request<void>(`/templates/${id}`, { method: 'DELETE' }),

  /** Instantiate a new project from a template. */
  createProject: (templateId: string, data: CreateProjectFromTemplate) =>
    request<{ id: string }>(`/projects/from-template/${templateId}`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  /** Snapshot a project's configuration (vocab, workflow, fields, boards) into a new template. */
  saveProjectAsTemplate: (projectId: string, data: { name: string; description?: string | null }) =>
    request<ProjectTemplate>(`/projects/${projectId}/save-as-template`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
};
