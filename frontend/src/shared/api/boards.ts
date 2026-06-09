import { request } from './client';
import type { Board, CreateBoard, UpdateBoard, BoardViewResponse } from '../types';

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
};
