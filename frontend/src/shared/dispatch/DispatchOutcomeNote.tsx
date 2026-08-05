import { type Component, Show } from 'solid-js';
import Badge from '../ui/Badge';
import { describeDispatchOutcome } from './format';

export interface DispatchOutcomeNoteProps {
  /** `DispatchItemResponse.outcome` or `SprintDispatchItemResponse.decision`
   *  — the two wire fields use different names for the same concept (see
   *  `./api.ts`'s header comment), so callers normalize to this one prop
   *  rather than this component knowing about either response shape. */
  decision: string;
  /** The "why" behind the decision — pass `dispatchOutcomeDetail(res)` or
   *  `sprintItemDetail(item)` from `./format.ts`. `null`/omitted when there's
   *  nothing more informative to say than the decision label itself. */
  detail?: string | null;
  /** Item title — only needed when rendering inside a list of many items
   *  (the sprint dispatch results); the item-detail drawer already names the
   *  item via the rest of the page, so it omits this. */
  title?: string;
}

/**
 * The single shared "here's what happened" renderer for a dispatch decision —
 * used by the item-detail dispatch control and the sprint-dispatch dry-run
 * preview / results list, so the outcome taxonomy (queued / policy-blocked /
 * waiting-approval / waiting-on-dependencies / would-dispatch / error / …)
 * reads identically everywhere rather than each call site inventing its own
 * copy. Never renders color alone (WCAG 1.4.1) — every state has a text
 * label from `describeDispatchOutcome`, plus a detail string naming the
 * actual reason when one exists (which policy blocked it, what status was or
 * wasn't applied, how many dependencies are still open).
 */
const DispatchOutcomeNote: Component<DispatchOutcomeNoteProps> = (props) => {
  const desc = () => describeDispatchOutcome(props.decision);

  return (
    <div class="flex flex-wrap items-center gap-2 text-sm">
      <Show when={props.title}>
        <span class="truncate font-medium" style={{ color: 'var(--color-text-primary)' }}>
          {props.title}
        </span>
      </Show>
      <Badge tone={desc().tone}>{desc().label}</Badge>
      <Show when={props.detail}>
        <span class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          {props.detail}
        </span>
      </Show>
    </div>
  );
};

export default DispatchOutcomeNote;
