import { createResource } from 'solid-js';
import { api } from '../../shared/api';
import { keyOf } from '../../shared/api/folder';
import type { Item } from '../../shared/types';

/** The plan fields the project folder holds for an item, by the first segment of a change's path. */
export type PlanField = 'title' | 'type' | 'parent' | 'description' | 'blockers' | 'roles' | 'fields' | 'brief';

const PATHS: Record<PlanField, string[]> = {
  title: ['title'],
  type: ['type', 'item_type'],
  parent: ['parent', 'parent_id'],
  description: ['description'],
  blockers: ['blockers', 'depends_on'],
  roles: ['roles'],
  fields: ['fields'],
  brief: ['brief'],
};

export interface FolderMarks {
  /** The revision the folder stands at; null when the project has no folder or no revision. */
  revision: () => number | null;
  /** What to say about these plan fields: nothing, that the folder holds them, or that the board changed them since the revision. */
  say: (...fields: PlanField[]) => { kind: 'held' } | { kind: 'changed'; revision: number } | null;
}

/** Whether the item's plan fields live in a project folder, and which of them the board changed since its last revision. */
export function useFolderMarks(item: () => Item): FolderMarks {
  // A 404 or any error means the project has no folder: nothing is said.
  const [status] = createResource(
    () => item().project_id,
    (id) => api.folder.status(id).catch(() => null),
  );
  const revision = () => status()?.last_revision?.number ?? null;
  return {
    revision,
    say: (...fields) => {
      const at = revision();
      if (at === null) return null;
      const key = keyOf(item());
      const names = fields.flatMap((f) => PATHS[f]);
      const changed = (status()?.changes ?? []).some(
        (c) => c.section === 'items' && c.key === key && c.fields.some((f) => names.includes(f.path.split('.')[0])),
      );
      return changed ? { kind: 'changed', revision: at } : { kind: 'held' };
    },
  };
}
