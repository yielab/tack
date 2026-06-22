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

  /** Import items from a CSV file into an existing project. */
  importCsv: (projectId: string, csvText: string) =>
    request<{ created: number; skipped: number }>(`/projects/${projectId}/import-csv`, {
      method: 'POST',
      headers: { 'Content-Type': 'text/csv' },
      body: csvText,
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

  // ── Cloud (external) backup ──────────────────────────────────────────────

  /** Read the cloud-backup configuration (secret key is never returned). */
  getCloudConfig: () => request<CloudBackupConfig>('/settings/backup'),

  /**
   * Save the cloud-backup configuration. Leave `secret_key` blank/undefined to
   * keep the currently stored secret unchanged.
   */
  saveCloudConfig: (config: CloudBackupConfigInput) =>
    request<CloudBackupConfig>('/settings/backup', {
      method: 'PUT',
      body: JSON.stringify(config),
    }),

  /** Trigger a backup to the configured cloud store and return its manifest. */
  cloudBackupNow: () =>
    request<RemoteBackupManifest>('/backup/remote', { method: 'POST' }),

  /** List existing cloud backups, newest first. */
  cloudBackups: () => request<RemoteBackupManifest[]>('/backup/remote'),

  /**
   * Restore from a cloud backup. Omit `key` to use the latest. Staged for the
   * next restart (a restart is required to apply it).
   */
  cloudRestore: (key?: string) =>
    request<{ staged: boolean; object_key: string; message: string }>(
      '/backup/remote/restore',
      { method: 'POST', body: JSON.stringify(key ? { key } : {}) },
    ),
};

/** Cloud-backup config as returned by the API (secret masked to a boolean). */
export interface CloudBackupConfig {
  configured: boolean;
  endpoint: string | null;
  bucket: string | null;
  region: string;
  access_key: string | null;
  secret_key_set: boolean;
  prefix: string;
  retention: number;
}

/** Fields accepted when saving cloud config. */
export interface CloudBackupConfigInput {
  endpoint?: string;
  bucket?: string;
  region?: string;
  access_key?: string;
  secret_key?: string;
  prefix?: string;
  retention?: number;
}

/** A single cloud backup's metadata. */
export interface RemoteBackupManifest {
  created_at: string;
  migration_version: number;
  item_count: number;
  object_key: string;
  bundle_size_bytes: number;
}
