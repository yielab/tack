import { createContext, createResource, useContext, type ParentComponent, type Accessor } from 'solid-js';
import { useParams } from '@solidjs/router';
import { api } from '../api';
import type { Item } from '../types';

interface ProjectItemsContextValue {
  items: Accessor<Item[] | undefined>;
  /** True only while there are no items for this project yet — the first
   *  load, or a switch to another project. A refetch keeps what is shown. */
  loading: Accessor<boolean>;
  refetch: () => void;
}

const ProjectItemsContext = createContext<ProjectItemsContextValue>();

/** The items and the project they belong to, so a refetch of the same project
 *  can be told from a switch to another one. */
interface LoadedItems {
  id: string;
  items: Item[];
}

/**
 * The new list, reusing the previous object for every item whose content did
 * not change. A view's `<For>` keys rows by object, so an unchanged item keeps
 * its row — and whatever that row holds open, like a "Run with agent" dialog —
 * across the refetch every realtime event triggers.
 */
export function shareUnchanged(previous: Item[] | undefined, next: Item[]): Item[] {
  if (!previous) return next;
  const before = new Map(previous.map((item) => [item.id, item]));
  return next.map((item) => {
    const old = before.get(item.id);
    return old && JSON.stringify(old) === JSON.stringify(item) ? old : item;
  });
}

export const ProjectItemsProvider: ParentComponent = (props) => {
  const params = useParams();
  const [resource, { refetch }] = createResource<LoadedItems, string>(
    () => params.id,
    async (id, info) => {
      const previous = info.value;
      const items = await api.items.list(id);
      return { id, items: previous?.id === id ? shareUnchanged(previous.items, items) : items };
    },
  );

  const value: ProjectItemsContextValue = {
    items: () => resource()?.items,
    loading: () => resource.loading && resource.latest?.id !== params.id,
    refetch,
  };

  return (
    <ProjectItemsContext.Provider value={value}>
      {props.children}
    </ProjectItemsContext.Provider>
  );
};

export function useProjectItems(): ProjectItemsContextValue {
  const ctx = useContext(ProjectItemsContext);
  if (!ctx) throw new Error('useProjectItems must be used within ProjectItemsProvider');
  return ctx;
}
