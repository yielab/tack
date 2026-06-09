// TEMPORARY COMPATIBILITY SHIM (T-501).
//
// The canonical API surface now lives in `shared/api` (`api.projects.list()`,
// `api.items.update()`, …). This file preserves the old *flat* method names
// (`api.listProjects()`, `api.getProject()`, …) so files not yet migrated keep
// compiling, while routing every call through the single `shared/api/client`
// (base URL, auth header, error handling).
//
// All callers are moved to `shared/api` during the T-503 restructure, at which
// point this shim is deleted.

import { api as core } from '../shared/api';
import type {
  Project,
  CreateProject,
  UpdateProject,
  Item,
  CreateItem,
  UpdateItem,
  BoardState,
  Sprint,
} from '../types/api';

export const api = {
  // Projects
  listProjects: (): Promise<Project[]> => core.projects.list(),
  getProject: (id: string): Promise<Project> => core.projects.get(id),
  createProject: (data: CreateProject): Promise<{ id: string }> =>
    core.projects.create(data),
  updateProject: (id: string, data: UpdateProject): Promise<Project> =>
    core.projects.update(id, data),
  deleteProject: (id: string): Promise<void> => core.projects.remove(id),

  // Items
  listItems: (projectId: string): Promise<Item[]> => core.items.list(projectId),
  getItem: (id: string): Promise<Item> => core.items.get(id),
  createItem: (projectId: string, data: CreateItem): Promise<{ id: string }> =>
    core.items.create(projectId, data),
  updateItem: (id: string, data: UpdateItem): Promise<Item> =>
    core.items.update(id, data),
  deleteItem: (id: string): Promise<void> => core.items.remove(id),

  // Board — resolves the project's default board and returns its view as the
  // legacy `BoardState` shape. The dead `GET /projects/{id}/board` route is
  // gone (T-502); this composes the real `/projects/{id}/boards` +
  // `/boards/{id}/view` endpoints. Board rendering moves onto the item store in
  // T-513.
  getBoard: async (projectId: string): Promise<BoardState> => {
    const list = await core.boards.list(projectId);
    const def = list.find((b) => b.is_default) ?? list[0];
    if (!def) return { columns: [] };
    const view = await core.boards.view(def.id);
    return {
      columns: view.columns.map((c) => ({
        status: c.name,
        items: c.items,
        wip_limit: c.wip_limit,
        wip_exceeded: c.wip_exceeded,
      })),
    };
  },

  // Sprints
  listSprints: (projectId: string): Promise<Sprint[]> =>
    core.sprints.list(projectId),

  // Search
  searchGlobal: (query: string, workspaceId?: string): Promise<Item[]> =>
    core.search.global(query, workspaceId),
  searchProject: (projectId: string, query: string): Promise<Item[]> =>
    core.search.inProject(projectId, query),
};
