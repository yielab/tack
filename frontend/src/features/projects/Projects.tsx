import { createResource, For, Show, createSignal, type Component } from 'solid-js';
import { A } from '@solidjs/router';
import { api } from '../../shared/api';
import CreateProjectModal from './CreateProjectModal';
import { Button, Skeleton } from '../../shared/ui';
import { IconBoard, IconPlus } from '../../shared/ui/icons';
import { projectTypePillStyle } from '../../shared/ui/projectTypeTone';

/** Six card-shaped placeholders in the loaded grid's container shape. */
const ProjectsGridSkeleton: Component = () => (
  <div class="grid grid-cols-1 gap-[18px] md:grid-cols-2 lg:grid-cols-3" aria-hidden="true">
    <For each={[1, 2, 3, 4, 5, 6]}>
      {() => (
        <div class="flex h-[150px] flex-col gap-2.5 rounded-[28px] bg-panel p-[18px]">
          <Skeleton width="70%" height="16px" />
          <Skeleton width="90%" height="10px" />
          <Skeleton width="60%" height="10px" />
          <div class="mt-auto">
            <Skeleton width="40%" height="10px" />
          </div>
        </div>
      )}
    </For>
  </div>
);

const Projects: Component = () => {
  const [projects, { refetch }] = createResource(() => api.projects.list());
  const [showCreateModal, setShowCreateModal] = createSignal(false);

  const handleProjectCreated = () => {
    refetch();
  };

  return (
    <div class="flex flex-col gap-[26px] lg:px-6 lg:py-3">
      <div class="flex flex-wrap items-end gap-4">
        <div>
          <h1 class="text-[34px] leading-tight text-content">Projects</h1>
          <p class="mt-1 text-content-muted">
            Select a project to get started
          </p>
        </div>
        <Button class="ml-auto" onClick={() => setShowCreateModal(true)}>
          <IconPlus size={14} />
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
            <div class="flex flex-col items-start gap-4 py-8">
              <div
                class="grid h-[120px] w-[120px] place-items-center rounded-full"
                style={{ background: 'var(--color-accent-soft)', color: 'var(--color-accent-ink)' }}
                aria-hidden="true"
              >
                <IconBoard size={44} />
              </div>
              <h2 class="max-w-[520px] text-2xl text-content">
                Track any kind of work — your terms, your workflow
              </h2>
              <p class="max-w-[520px] text-content-muted">
                Tack adapts to software teams, construction projects, personal tasks, and more.
                Create your first project to get started.
              </p>
              <div class="flex flex-wrap items-center gap-2.5">
                <Button size="lg" onClick={() => setShowCreateModal(true)}>
                  Create your first project
                </Button>
                <A
                  href="/templates"
                  class="rounded-full px-3 py-2 text-sm font-semibold text-accent-ink underline-offset-4 hover:underline focus:outline-none focus-visible:ring-2"
                  style={{ '--tw-ring-color': 'var(--color-focus-ring)' }}
                >
                  Browse templates
                </A>
              </div>
            </div>
          }
        >
          <div class="grid grid-cols-1 gap-[18px] md:grid-cols-2 lg:grid-cols-3">
            <For each={projects()}>
              {(project) => (
                <A
                  href={`/projects/${project.id}/board`}
                  class="flex min-h-[170px] flex-col gap-2.5 rounded-[28px] border-2 border-transparent bg-panel p-5 transition-[border-color,box-shadow] hover:border-brand hover:shadow-[var(--shadow-md)] focus:outline-none focus-visible:border-brand"
                >
                  <div class="flex items-start gap-2.5">
                    <h3 class="min-w-0 text-[21px] leading-[1.15] text-content">
                      {project.name}
                    </h3>
                    <span class="ml-auto shrink-0" style={projectTypePillStyle(project.project_type)}>
                      {project.project_type}
                    </span>
                  </div>
                  <Show when={project.description}>
                    <p class="line-clamp-2 text-[13.5px] leading-[1.45] text-content-muted">
                      {project.description}
                    </p>
                  </Show>
                  <div class="mt-auto border-t border-line pt-3">
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
