import { type Component, Show } from 'solid-js';
import { A } from '@solidjs/router';
import { Badge } from '../ui';

export type PrerequisiteState = 'ok' | 'missing' | 'deferred';

export interface PrerequisiteProps {
  state: PrerequisiteState;
  label: string;
  /** `missing`: the page that fixes it. */
  href?: string;
  /** `missing`: the link text. */
  fixLabel?: string;
  /** `deferred`: one line saying why, and what enables it. */
  reason?: string;
  /** `ok` with `onToggle`: a live checkbox the operator can untick to decline this step. */
  checked?: boolean;
  onToggle?: (on: boolean) => void;
}

/**
 * One prerequisite row of the run flow. `ok` is a badge, `missing` links to
 * the page that fixes it, `deferred` is a disabled control with its reason —
 * a deferred capability is shown, never hidden.
 */
const Prerequisite: Component<PrerequisiteProps> = (props) => (
  <div
    class="space-y-1 rounded-[20px] px-4 py-2.5 text-sm"
    style={{ 'background-color': 'var(--color-bg-app)', color: 'var(--color-text-primary)' }}
    data-prerequisite={props.state}
  >
    <Show
      when={props.state === 'deferred'}
      fallback={
        <div class="flex flex-wrap items-center justify-between gap-2">
          <Show
            when={props.state === 'ok' && props.onToggle}
            fallback={
              <span class="flex items-center gap-2">
                <Badge tone={props.state === 'ok' ? 'success' : 'danger'}>{props.state === 'ok' ? 'Ready' : 'Missing'}</Badge>
                {props.label}
              </span>
            }
          >
            <label class="flex cursor-pointer items-center gap-2.5">
              <input
                type="checkbox"
                class="h-4 w-4"
                style={{ 'accent-color': 'var(--color-primary-600)' }}
                checked={props.checked ?? true}
                onChange={(e) => props.onToggle?.(e.currentTarget.checked)}
              />
              {props.label}
            </label>
          </Show>
          <Show when={props.state === 'missing' && props.href}>
            <A
              href={props.href!}
              class="text-xs font-semibold hover:underline"
              style={{ color: 'var(--color-accent-ink)' }}
            >
              {props.fixLabel ?? 'Set it up'}
            </A>
          </Show>
        </div>
      }
    >
      <label class="flex cursor-not-allowed items-center gap-2.5 opacity-60">
        <input type="checkbox" class="h-4 w-4" disabled />
        {props.label}
      </label>
      <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
        {props.reason}
      </p>
    </Show>
  </div>
);

export default Prerequisite;
