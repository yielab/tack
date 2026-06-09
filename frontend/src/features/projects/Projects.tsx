import { createResource, For, Show, createSignal, type Component } from 'solid-js';
import { A } from '@solidjs/router';
import { FiPlus, FiFolder } from 'solid-icons/fi';
import { api } from '../../shared/api';
import CreateProjectModal from './CreateProjectModal';
import { ProjectsGridSkeleton } from '../../shared/ui/SkeletonScreen';

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
          <h1 class="text-3xl font-bold text-gray-900 dark:text-white">Projects</h1>
          <p class="mt-2 text-gray-600 dark:text-gray-400">
            Select a project to get started
          </p>
        </div>
        <button
          onClick={() => setShowCreateModal(true)}
          class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 flex items-center gap-2 transition-colors"
        >
          <FiPlus />
          New Project
        </button>
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
            <div class="text-center py-12">
              <FiFolder size={48} class="mx-auto text-gray-400 mb-4" />
              <h3 class="text-lg font-medium text-gray-900 dark:text-white mb-2">
                No projects yet
              </h3>
              <p class="text-gray-600 dark:text-gray-400">
                Get started by creating your first project
              </p>
            </div>
          }
        >
          <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            <For each={projects()}>
              {(project) => (
                <A
                  href={`/board?project=${project.id}`}
                  class="block p-6 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 hover:border-purple-500 dark:hover:border-purple-500 transition-colors"
                >
                  <div class="flex items-start justify-between mb-4">
                    <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                      {project.name}
                    </h3>
                    <span class="px-2 py-1 text-xs font-medium bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 rounded">
                      {project.project_type}
                    </span>
                  </div>
                  <Show when={project.description}>
                    <p class="text-gray-600 dark:text-gray-400 text-sm line-clamp-2">
                      {project.description}
                    </p>
                  </Show>
                  <div class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700">
                    <p class="text-xs text-gray-500 dark:text-gray-500">
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
