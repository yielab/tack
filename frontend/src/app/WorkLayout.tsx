import { Show, type ParentComponent } from 'solid-js';
import WorkTabs from '../shared/ui/WorkTabs';
import LoadError from '../shared/ui/LoadError';
import { ProjectItemsProvider, useProjectItems } from '../shared/state/projectItemsContext';

/** The lens, or why there are no items to draw: a failed load is not an empty project. */
const Lens: ParentComponent = (props) => {
  const { error, refetch } = useProjectItems();
  return (
    <Show when={!error()} fallback={<LoadError what="the items" error={error()} onRetry={refetch} />}>
      {props.children}
    </Show>
  );
};

/** Wrapper for the 5 "same items, different lens" routes.
 *  Fetches items once and shares them via context so tab-switching never
 *  triggers a redundant API call. */
const WorkLayout: ParentComponent = (props) => {
  return (
    <ProjectItemsProvider>
      <div class="flex flex-col gap-4 flex-1 min-h-0 min-w-0">
        <WorkTabs />
        <Lens>{props.children}</Lens>
      </div>
    </ProjectItemsProvider>
  );
};

export default WorkLayout;
