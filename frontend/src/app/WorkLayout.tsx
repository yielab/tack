import { type ParentComponent } from 'solid-js';
import WorkTabs from '../shared/ui/WorkTabs';
import { ProjectItemsProvider } from '../shared/state/projectItemsContext';

/** Wrapper for the 5 "same items, different lens" routes.
 *  Fetches items once and shares them via context so tab-switching never
 *  triggers a redundant API call. */
const WorkLayout: ParentComponent = (props) => {
  return (
    <ProjectItemsProvider>
      <div class="flex flex-col gap-4">
        <WorkTabs />
        {props.children}
      </div>
    </ProjectItemsProvider>
  );
};

export default WorkLayout;
