import { type Component, For } from 'solid-js';

/**
 * Skeleton loader for board columns
 */
export const BoardSkeleton: Component = () => {
  const columns = [1, 2, 3, 4]; // Show 4 skeleton columns
  const cardsPerColumn = [3, 2, 4, 2]; // Varying card counts

  return (
    <div class="flex gap-4 overflow-x-auto pb-4">
      <For each={columns}>
        {(_col, index) => (
          <div class="flex-shrink-0 w-80">
            <div class="bg-sunken rounded-lg p-4 min-h-[500px] animate-pulse">
              {/* Column header */}
              <div class="flex items-center justify-between mb-4">
                <div class="h-6 bg-sunken rounded w-24"></div>
                <div class="flex items-center gap-2">
                  <div class="h-5 bg-sunken rounded w-8"></div>
                  <div class="h-6 bg-sunken rounded w-10"></div>
                </div>
              </div>

              {/* Cards */}
              <div class="space-y-3">
                <For each={Array(cardsPerColumn[index()])}>
                  {() => (
                    <div class="bg-surface rounded-lg p-4 shadow-sm border border-line">
                      {/* Card title */}
                      <div class="h-5 bg-sunken rounded w-3/4 mb-2"></div>
                      {/* Card description */}
                      <div class="h-4 bg-sunken rounded w-full mb-1"></div>
                      <div class="h-4 bg-sunken rounded w-5/6 mb-3"></div>
                      {/* Card badges */}
                      <div class="flex items-center gap-2">
                        <div class="h-6 bg-sunken rounded w-16"></div>
                        <div class="h-6 bg-sunken rounded w-12"></div>
                      </div>
                    </div>
                  )}
                </For>
              </div>

              {/* Add button skeleton */}
              <div class="mt-3 h-8 bg-sunken rounded w-full"></div>
            </div>
          </div>
        )}
      </For>
    </div>
  );
};

/**
 * Skeleton loader for project list grid
 */
export const ProjectsGridSkeleton: Component = () => {
  const projects = [1, 2, 3, 4, 5, 6]; // Show 6 skeleton cards

  return (
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
      <For each={projects}>
        {() => (
          <div class="bg-surface rounded-lg p-6 shadow-sm border border-line animate-pulse">
            {/* Project icon & title */}
            <div class="flex items-start gap-3 mb-4">
              <div class="w-10 h-10 bg-brand-300 rounded-lg flex-shrink-0"></div>
              <div class="flex-1">
                <div class="h-6 bg-sunken rounded w-3/4 mb-2"></div>
                <div class="h-4 bg-sunken rounded w-1/2"></div>
              </div>
            </div>

            {/* Description */}
            <div class="mb-4">
              <div class="h-4 bg-sunken rounded w-full mb-2"></div>
              <div class="h-4 bg-sunken rounded w-5/6"></div>
            </div>

            {/* Stats */}
            <div class="flex items-center gap-4 text-sm text-content-subtle">
              <div class="flex items-center gap-1">
                <div class="h-4 bg-sunken rounded w-16"></div>
              </div>
              <div class="flex items-center gap-1">
                <div class="h-4 bg-sunken rounded w-16"></div>
              </div>
            </div>
          </div>
        )}
      </For>
    </div>
  );
};

/**
 * Generic skeleton for list items
 */
export const ListSkeleton: Component<{ rows?: number }> = (props) => {
  const rows = props.rows || 5;

  return (
    <div class="space-y-3">
      <For each={Array(rows)}>
        {() => (
          <div class="bg-surface rounded-lg p-4 shadow-sm border border-line animate-pulse">
            <div class="flex items-center justify-between">
              <div class="flex-1">
                <div class="h-5 bg-sunken rounded w-1/2 mb-2"></div>
                <div class="h-4 bg-sunken rounded w-3/4"></div>
              </div>
              <div class="flex gap-2">
                <div class="h-8 w-20 bg-sunken rounded"></div>
                <div class="h-8 w-20 bg-sunken rounded"></div>
              </div>
            </div>
          </div>
        )}
      </For>
    </div>
  );
};

/**
 * Simple text skeleton
 */
export const TextSkeleton: Component<{ width?: string; height?: string }> = (props) => {
  return (
    <div
      class="bg-sunken rounded animate-pulse"
      style={{
        width: props.width || '100%',
        height: props.height || '1rem',
      }}
    ></div>
  );
};

export default {
  Board: BoardSkeleton,
  ProjectsGrid: ProjectsGridSkeleton,
  List: ListSkeleton,
  Text: TextSkeleton,
};
