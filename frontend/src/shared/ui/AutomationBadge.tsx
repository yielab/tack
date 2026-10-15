import { type Component, Show } from 'solid-js';
import Badge from './Badge';
import { HELP } from '../help/texts';

export interface AutomationBadgeProps {
  /** `metadata.started_by` of the task's newest request: `auto_start` when the project started it. */
  startedBy?: string | null;
  /** The team the newest request's profile names. */
  team?: string | null;
  /** How the task's merge was triggered (`auto` when Tack merged by itself). */
  mergedBy?: string | null;
}

/**
 * What Tack did by itself on a task: "Started by itself", "Team: {name}", "Merged by itself".
 * Renders nothing when it did none of them. Each badge carries its help text as its title.
 */
const AutomationBadge: Component<AutomationBadgeProps> = (props) => (
  <>
    <Show when={props.startedBy === 'auto_start'}>
      <span data-testid="badge-started-by-itself" title={HELP.startedByItself.text}><Badge tone="info">Started by itself</Badge></span>
    </Show>
    <Show when={props.team}>
      {(team) => <span data-testid="badge-team" title={HELP.teamBadge.text}><Badge tone="neutral">Team: {team()}</Badge></span>}
    </Show>
    <Show when={props.mergedBy === 'auto'}>
      <span data-testid="badge-merged-by-itself" title={HELP.mergedByItself.text}><Badge tone="success">Merged by itself</Badge></span>
    </Show>
  </>
);

export default AutomationBadge;
