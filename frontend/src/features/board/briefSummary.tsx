import { Show, type Component } from 'solid-js';
import { FiAlertTriangle } from 'solid-icons/fi';
import type { Item } from '../../shared/types';
import Badge from '../../shared/ui/Badge';

/** What the project's item list says about a task's brief; the API writes the reason, the screen only shows it. */
export interface BriefSummary {
  done_when: boolean;
  checks: number;
  risk: 'low' | 'medium' | 'high' | null;
  not_ready: string | null;
}

/** An item as the project's list serves it: the brief's summary rides along when the API has one. */
export type ItemWithBrief = Item & { brief_summary?: BriefSummary | null };

export const briefOf = (item: Item): BriefSummary | null => (item as ItemWithBrief).brief_summary ?? null;

/** The reason a task cannot be verified yet, as the API gave it. */
export const notReadyOf = (item: Item): string | null => briefOf(item)?.not_ready || null;

/** Whether any item of the list carries a brief summary at all. */
export const listsBriefs = (items: readonly Item[]): boolean => items.some((item) => briefOf(item) != null);

/** "Not ready: <reason>": a warning in amber text, never a fill, and never a block on running the task. */
export const NotReadyLine: Component<{ reason: string; class?: string }> = (props) => (
  <p
    data-testid="not-ready"
    class={`m-0 inline-flex min-w-0 items-start gap-1.5 text-small font-semibold ${props.class ?? ''}`}
    style={{ color: 'var(--color-warning-700)', 'overflow-wrap': 'anywhere' }}
  >
    <FiAlertTriangle size={13} aria-hidden="true" class="mt-0.5 flex-none" />
    <span class="min-w-0">Not ready: {props.reason}</span>
  </p>
);

/** "Risk: high" beside the type, only for a risk a person should weigh (medium or high). */
export const RiskChip: Component<{ item: Item }> = (props) => {
  const risk = () => {
    const level = briefOf(props.item)?.risk;
    return level === 'medium' || level === 'high' ? level : null;
  };
  return (
    <Show when={risk()}>
      {(level) => (
        <span data-testid="risk-chip">
          <Badge tone={level() === 'high' ? 'warning' : 'neutral'}>Risk: {level()}</Badge>
        </span>
      )}
    </Show>
  );
};
