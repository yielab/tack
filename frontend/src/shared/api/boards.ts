import { request } from './client';
import type {
  Board,
  CreateBoard,
  UpdateBoard,
  BoardViewResponse,
  BoardState,
} from '../types';

export const boards = {
  /** All boards configured for a project. */
  list: (projectId: string) =>
    request<Board[]>(`/projects/${projectId}/boards`),

  get: (boardId: string) => request<Board>(`/boards/${boardId}`),

  /** Column/item view for a single board (the data the Board UI renders).
   * Columns use `name`; callers map it onto the `BoardState.status` shape. */
  view: (boardId: string) =>
    request<BoardViewResponse>(`/boards/${boardId}/view`),

  create: (projectId: string, data: CreateBoard) =>
    request<Board>(`/projects/${projectId}/boards`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  update: (boardId: string, data: UpdateBoard) =>
    request<Board>(`/boards/${boardId}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    }),

  remove: (boardId: string) =>
    request<void>(`/boards/${boardId}`, { method: 'DELETE' }),

  /**
   * Convenience: resolve a project's default board (or first) and return its
   * view mapped onto the legacy `BoardState` shape (`columns[].status`). Used by
   * the Board page until the item store lands in T-513. Composes existing routes
   * only — no new endpoint.
   */
  projectBoardState: async (projectId: string): Promise<BoardState> => {
    const list = await boards.list(projectId);
    const def = list.find((b) => b.is_default) ?? list[0];
    if (!def) return { columns: [] };
    const view = await boards.view(def.id);
    return {
      columns: view.columns.map((c) => ({
        status: c.name,
        items: c.items,
        wip_limit: c.wip_limit,
        wip_exceeded: c.wip_exceeded,
      })),
    };
  },
};
