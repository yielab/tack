import { createResource, For, Show, createSignal, type Component } from 'solid-js';
import { A } from '@solidjs/router';
import { FiPlus } from 'solid-icons/fi';
import { api } from '../../shared/api';
import CreateProjectModal from './CreateProjectModal';
import { ProjectsGridSkeleton } from '../../shared/ui/SkeletonScreen';
import { Button } from '../../shared/ui';

const Projects: Component = () => {
  const [projects, { refetch }] = createResource(() => api.projects.list());
  const [showCreateModal, setShowCreateModal] = createSignal(false);

  const handleProjectCreated = () => {
    refetch();
  };

  return (
    <div>
      <div class="flex items-center justify-between mb-8">
        <div>
          <h1 class="text-3xl font-bold text-content">Projects</h1>
          <p class="mt-2 text-content-muted">
            Select a project to get started
          </p>
        </div>
        <Button onClick={() => setShowCreateModal(true)}>
          <FiPlus />
          New Project
        </Button>
      </div>

      <CreateProjectModal
        isOpen={showCreateModal()}
        onClose={() => setShowCreateModal(false)}
        onSuccess={handleProjectCreated}
      />

      <Show
        when={!projects.loading}
        fallback={<ProjectsGridSkeleton />}
      >
        <Show
          when={projects() && projects()!.length > 0}
          fallback={
            <div class="flex flex-col items-center justify-center py-20 px-4 text-center">
              <div class="text-6xl mb-6" aria-hidden="true">📋</div>
              <h2 class="text-2xl font-bold mb-2" style={{ color: 'var(--color-text-primary)' }}>
                Track any kind of work — your terms, your workflow
              </h2>
              <p class="max-w-md mb-8" style={{ color: 'var(--color-text-secondary)' }}>
                FlexPM adapts to software teams, construction projects, personal tasks, and more.
                Create your first project to get started.
              </p>
              <div class="flex flex-col sm:flex-row gap-3">
                <Button size="lg" onClick={() => setShowCreateModal(true)}>
                  Create your first project
                </Button>
                <A
                  href="/templates"
                  class="inline-flex items-center justify-center px-4 py-2 text-sm font-medium rounded-lg border transition-colors"
                  style={{
                    color: 'var(--color-text-secondary)',
                    'border-color': 'var(--color-border-medium)',
                    'background-color': 'var(--color-bg-base)',
                  }}
                >
                  Browse templates
                </A>
              </div>
            </div>
          }
        >
          <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            <For each={projects()}>
              {(project) => (
                <A
                  href={`/projects/${project.id}/board`}
                  class="block p-6 bg-elevated rounded-lg border border-line hover:border-brand-500 transition-colors"
                >
                  <div class="flex items-start justify-between mb-4">
                    <h3 class="text-lg font-semibold text-content">
                      {project.name}
                    </h3>
                    <span class="px-2 py-1 text-xs font-medium bg-brand-100 text-brand-700 rounded">
                      {project.project_type}
                    </span>
                  </div>
                  <Show when={project.description}>
                    <p class="text-content-muted text-sm line-clamp-2">
                      {project.description}
                    </p>
                  </Show>
                  <div class="mt-4 pt-4 border-t border-line">
                    <p class="text-xs text-content-subtle">
                      Created {new Date(project.created_at).toLocaleDateString()}
                    </p>
                  </div>
                </A>
              )}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
};

export default Projects;
