import { request } from './client';
import type { BoardState, Item, Project } from '../types';

/** Pure function: derive Kanban columns from a project's workflow + items list. */
export function deriveBoard(project: Project, items: Item[]): BoardState {
  const statuses = [...project.workflow.statuses].sort((a, b) => a.order - b.order);
  const byStatus = new Map<string, Item[]>(statuses.map((s) => [s.name, []]));
  for (const item of items) {
    const col = byStatus.get(item.status);
    if (col) col.push(item);
    else if (statuses.length > 0) byStatus.get(statuses[0].name)!.push(item);
  }
  return {
    columns: statuses.map((s) => {
      const colItems = byStatus.get(s.name) ?? [];
      return {
        status: s.name,
        items: colItems,
        wip_limit: s.wip_limit,
        wip_exceeded: s.wip_limit != null && colItems.length > s.wip_limit,
      };
    }),
  };
}

export const boards = {
  projectBoardState: async (projectId: string): Promise<BoardState> => {
    const [project, allItems] = await Promise.all([
      request<Project>(`/projects/${projectId}`),
      request<Item[]>(`/projects/${projectId}/items`),
    ]);
    return deriveBoard(project, allItems);
  },
};
