import { request } from './client';
import type { components } from './schema.gen';

type Schemas = components['schemas'];

/** The project folder as one object: a path relative to `.tack/` and its text. */
export type FolderFiles = Record<string, string>;

export type RevisionSummary = Schemas['RevisionSummary'];
/** The origins a revision has: an export of the board, an import of the folder, or a revert. */
export type RevisionOrigin = 'board' | 'file' | 'revert';
export type Verb = Schemas['Verb'];
export type FieldChange = Schemas['FieldChange'];
/** One entry of one section and what an apply does to it. */
export type Change = Schemas['Change'];
/** A field the folder and the board both changed since the last revision. */
export type Conflict = Schemas['Conflict'];
/** What a removal deletes with it, one line each. */
export type Removal = Schemas['Removal'];
export type FolderStatus = Schemas['FolderStatus'];
/** What an apply would do; `applies` is false while a conflict has no side to take. */
export type FolderPlan = Schemas['FolderPlan'];
export type FolderApplied = Schemas['FolderApplied'];

/** When both sides changed a field: ask (no side), or take one side for every conflict. */
export type Prefer = Schemas['Prefer'];

/** What a plan reads: the folder's files, or a revision to go back to. */
export type PlanSource = { files: FolderFiles } | { revision: number };

function planQuery(source: PlanSource, prefer?: Prefer, planHash?: string): string {
  const query = new URLSearchParams();
  if ('revision' in source) query.set('revision', String(source.revision));
  if (prefer) query.set('prefer', prefer);
  if (planHash) query.set('plan_hash', planHash);
  const text = query.toString();
  return text ? `?${text}` : '';
}

const planBody = (source: PlanSource): RequestInit =>
  'files' in source ? { method: 'POST', body: JSON.stringify({ files: source.files }) } : { method: 'POST' };

/** The project folder: export, status, plan, apply and the revisions. */
export const folder = {
  /** The folder as the board has it. Records a revision when the board changed since the last. */
  export: (projectId: string) => request<{ files: FolderFiles }>(`/projects/${projectId}/folder`),
  status: (projectId: string) => request<FolderStatus>(`/projects/${projectId}/folder/status`),
  /** What an apply would do. Nothing is written. */
  plan: (projectId: string, source: PlanSource, prefer?: Prefer) =>
    request<FolderPlan>(`/projects/${projectId}/folder/plan${planQuery(source, prefer)}`, planBody(source)),
  /** Applies the plan whose hash the preview answered; a board changed since then answers 409. */
  apply: (projectId: string, source: PlanSource, planHash: string, prefer?: Prefer) =>
    request<FolderApplied>(`/projects/${projectId}/folder/apply${planQuery(source, prefer, planHash)}`, planBody(source)),
  /** The revisions, newest first. */
  revisions: (projectId: string) => request<RevisionSummary[]>(`/projects/${projectId}/folder/revisions`),
  revision: (projectId: string, n: number) =>
    request<{ files: FolderFiles }>(`/projects/${projectId}/folder/revisions/${n}`),
};

/** A row's key, or nothing for one that has none (a thread's or a trigger's hidden item has ""). */
export function keyOf(row: { key?: string | null }): string | undefined {
  return row.key ? row.key : undefined;
}

/**
 * Where each file of a picked folder goes, by its path inside `.tack/` (`webkitRelativePath`
 * starts with the picked folder's own name). When the pick holds a `.tack/` folder (the
 * repository was picked), only what is under it is read; otherwise the picked folder is `.tack/`.
 */
export function folderPaths(relativePaths: string[]): Map<string, string> {
  const split = relativePaths.map((path) => ({ path, parts: path.split('/').filter((p) => p !== '') }));
  const holdsTack = split.some(({ parts }) => parts.slice(0, -1).includes('.tack'));
  const paths = new Map<string, string>();
  for (const { path, parts } of split) {
    const inside = holdsTack ? parts.slice(parts.indexOf('.tack') + 1) : parts.slice(1);
    if ((holdsTack && !parts.slice(0, -1).includes('.tack')) || inside.length === 0) continue;
    paths.set(path, inside.join('/'));
  }
  return paths;
}
