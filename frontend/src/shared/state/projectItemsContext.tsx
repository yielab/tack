import { createContext, createResource, useContext, type ParentComponent, type Accessor } from 'solid-js';
import { useParams } from '@solidjs/router';
import { api } from '../api';
import type { Item } from '../types';

interface ProjectItemsContextValue {
  items: Accessor<Item[] | undefined>;
  loading: Accessor<boolean>;
  refetch: () => void;
}

const ProjectItemsContext = createContext<ProjectItemsContextValue>();

export const ProjectItemsProvider: ParentComponent = (props) => {
  const params = useParams();
  const [resource, { refetch }] = createResource(
    () => params.id,
    (id) => api.items.list(id),
  );

  const value: ProjectItemsContextValue = {
    items: () => resource(),
    loading: () => resource.loading,
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
