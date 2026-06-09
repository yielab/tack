import { request, requestBlob, apiUrl } from './client';

export type ExportFormat = 'json' | 'csv';

/** Export / import (per project) and full-database backup / restore. */
export const data = {
  /** Download a project export (JSON snapshot or CSV item list). */
  exportProject: (projectId: string, format: ExportFormat = 'json') =>
    requestBlob(`/projects/${projectId}/export?format=${format}`),

  exportUrl: (projectId: string, format: ExportFormat = 'json') =>
    apiUrl(`/projects/${projectId}/export?format=${format}`),

  /** Import a project from a previously exported JSON snapshot. */
  importProject: (snapshot: unknown) =>
    request<{ id: string }>('/projects/import', {
      method: 'POST',
      body: JSON.stringify(snapshot),
    }),

  /** Download a full SQLite database backup. */
  backup: () => requestBlob('/backup'),

  backupUrl: () => apiUrl('/backup'),

  /**
   * Restore the database from a backup file. The endpoint expects the raw
   * SQLite file bytes as the request body (NOT multipart form-data).
   */
  restore: (file: Blob) =>
    request<{ ok?: boolean }>('/restore', {
      method: 'POST',
      headers: { 'Content-Type': 'application/octet-stream' },
      body: file,
    }),
};
