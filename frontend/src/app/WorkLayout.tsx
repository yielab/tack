import { type ParentComponent } from 'solid-js';
import WorkTabs from '../shared/ui/WorkTabs';

/** Wrapper for the 5 "same items, different lens" routes.
 *  Renders the WorkTabs control above the active lens view. */
const WorkLayout: ParentComponent = (props) => {
  return (
    <div class="flex flex-col gap-4">
      <WorkTabs />
      {props.children}
    </div>
  );
};

export default WorkLayout;
