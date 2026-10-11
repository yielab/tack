import { request, requestBlob, apiUrl } from './client';
import type { components } from './schema.gen';

export type ExportFormat = 'json' | 'yaml' | 'csv';

/** What an outline or CSV import created, or would create on a dry run. */
export type OutlineResult = components['schemas']['OutlineResult'];

/** An import file: a CSV, or an outline as JSON or YAML. */
export type ImportFileKind = 'csv' | 'json' | 'yaml';

/** The kind of an import file, by its extension (JSON when it has neither). */
export function importFileKind(fileName: string): ImportFileKind {
  if (/\.csv$/i.test(fileName)) return 'csv';
  if (/\.ya?ml$/i.test(fileName)) return 'yaml';
  return 'json';
}

const IMPORT_CONTENT_TYPE: Record<ImportFileKind, string> = {
  csv: 'text/csv',
  json: 'application/json',
  yaml: 'application/x-yaml',
};

function importQuery(params: Record<string, string | boolean | undefined>): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== false) query.set(key, String(value));
  }
  const text = query.toString();
  return text ? `?${text}` : '';
}

/** Export / import (per project) and full-database backup / restore. */
export const data = {
  /** Download a project export (JSON snapshot or CSV item list). */
  exportProject: (projectId: string, format: ExportFormat = 'json') =>
    requestBlob(`/projects/${projectId}/export?format=${format}`),

  exportUrl: (projectId: string, format: ExportFormat = 'json') =>
    apiUrl(`/projects/${projectId}/export?format=${format}`),

  /** Import a project from a previously exported JSON snapshot. */
  importProject: (snapshot: unknown) =>
    request<ImportResult>('/projects/import', {
      method: 'POST',
      body: JSON.stringify(snapshot),
    }),

  /** Import a project from a previously exported YAML snapshot (raw text). */
  importProjectYaml: (yamlText: string) =>
    request<ImportResult>('/projects/import', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-yaml' },
      body: yamlText,
    }),

  /**
   * Add items to an existing project from a CSV or an outline. Nothing is
   * written unless the whole file can be; `dryRun` reports what would be.
   */
  importItems: (projectId: string, text: string, kind: ImportFileKind, dryRun = false) =>
    request<OutlineResult>(
      `/projects/${projectId}/${kind === 'csv' ? 'import-csv' : 'outline'}${importQuery({ dry_run: dryRun })}`,
      { method: 'POST', headers: { 'Content-Type': IMPORT_CONTENT_TYPE[kind] }, body: text },
    ),

  /**
   * Create a project with its items from a CSV or an outline. `name` is
   * required for a CSV and overrides an outline's own.
   */
  createProjectFromFile: (
    text: string,
    kind: ImportFileKind,
    options: { name?: string; dryRun?: boolean } = {},
  ) =>
    request<OutlineResult>(
      `/projects/${kind === 'csv' ? 'import-csv' : 'outline'}${importQuery({ name: options.name, dry_run: options.dryRun })}`,
      { method: 'POST', headers: { 'Content-Type': IMPORT_CONTENT_TYPE[kind] }, body: text },
    ),

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
   * next restart (a restart is required to apply it). Pass `force` to override
   * the "this device has newer work" guard.
   */
  cloudRestore: (key?: string, force = false) =>
    request<{ staged: boolean; object_key: string; message: string }>(
      '/backup/remote/restore',
      { method: 'POST', body: JSON.stringify({ ...(key ? { key } : {}), ...(force ? { force } : {}) }) },
    ),

  /**
   * Verify a cloud backup (download + checksum + schema check) WITHOUT staging
   * it. Omit `key` to verify the latest. Returns an ok/mismatch verdict.
   */
  cloudVerify: (key?: string) =>
    request<CloudVerifyResult>('/backup/remote/verify', {
      method: 'POST',
      body: JSON.stringify(key ? { key } : {}),
    }),
};

/** Result of verifying a cloud backup without staging it. */
export interface CloudVerifyResult {
  ok: boolean;
  object_key: string;
  manifest: RemoteBackupManifest;
  reason?: string;
}

/** Response from the project-import endpoint. */
export interface ImportResult {
  success: boolean;
  project: { id: string };
}

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
