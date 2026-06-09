import {
  createContext,
  useContext,
  createResource,
  type Accessor,
  type Resource,
  type ParentComponent,
} from 'solid-js';
import { useParams } from '@solidjs/router';
import { api } from '../api';
import type { Project, WorkflowConfig } from '../types';

export interface ProjectContextValue {
  /** Active project id from the route (`:id`), or undefined off a project route. */
  projectId: Accessor<string | undefined>;
  /** The active project resource (single shared fetch). */
  project: Resource<Project | null>;
  /** Convenience accessor for the project's workflow. */
  workflow: Accessor<WorkflowConfig | undefined>;
  /** Convenience accessor for the project's vocabulary map (reactive). */
  vocabulary: Accessor<Record<string, string> | undefined>;
  /** Re-fetch the active project (e.g. after a settings save). */
  refetch: () => void;
}

/** Exported for tests that supply a context value directly. App code should
 * use {@link ProjectProvider} / {@link useProject}. */
export const ProjectContext = createContext<ProjectContextValue>();

/**
 * Holds the active project once, keyed by the route `:id`. Mounted at the app
 * root (Layout) so every project-scoped view shares a single fetch and reacts
 * to vocabulary/workflow changes without re-fetching per page.
 */
export const ProjectProvider: ParentComponent = (props) => {
  const params = useParams();
  const projectId = () => params.id as string | undefined;

  const [project, { refetch }] = createResource(
    projectId,
    (id) => (id ? api.projects.get(id) : null),
  );

  const value: ProjectContextValue = {
    projectId,
    project,
    workflow: () => project()?.workflow,
    vocabulary: () => project()?.vocabulary,
    refetch: () => {
      void refetch();
    },
  };

  return (
    <ProjectContext.Provider value={value}>
      {props.children}
    </ProjectContext.Provider>
  );
};

/** Access the active-project context. Throws if used outside ProjectProvider. */
export function useProject(): ProjectContextValue {
  const ctx = useContext(ProjectContext);
  if (!ctx) {
    throw new Error('useProject must be used within a ProjectProvider');
  }
  return ctx;
}
