import { Show, type Component, type JSX } from 'solid-js';
import clsx from 'clsx';

/** What the section is about, said by colour: done is green, the agent teal, people blue, what needs a person amber. */
export type SectionTone = 'neutral' | 'done' | 'agent' | 'people' | 'needs';

export interface SectionHeadingProps {
  /** The icon in the tinted square. It is decoration: the heading names the section. */
  icon: JSX.Element;
  tone?: SectionTone;
  /** How many things the section holds, when the number is already loaded. */
  count?: number;
  /** Buttons and links at the end of the row. */
  actions?: JSX.Element;
  /** The id of the heading, for a section's `aria-labelledby`. */
  id?: string;
  class?: string;
  /** The heading's text. */
  children: JSX.Element;
}

const TONE: Record<SectionTone, { background: string; color: string }> = {
  neutral: { background: 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' },
  done: { background: 'var(--color-success-100)', color: 'var(--color-success-700)' },
  agent: { background: 'var(--color-accent-soft)', color: 'var(--color-accent-ink)' },
  people: { background: 'var(--color-info-100)', color: 'var(--color-info-700)' },
  needs: { background: 'var(--color-warning-100)', color: 'var(--color-warning-700)' },
};

/** The heading of a section of a page: an icon in a 28px tinted square, the title at the section size,
 *  an optional count, and an optional slot for the section's actions at the end of the row. */
const SectionHeading: Component<SectionHeadingProps> = (props) => (
  <div class={clsx('flex min-w-0 items-center gap-3', props.class)}>
    <span
      aria-hidden="true"
      data-testid="section-icon"
      class="grid h-7 w-7 flex-none place-items-center rounded-md"
      style={{ 'background-color': TONE[props.tone ?? 'neutral'].background, color: TONE[props.tone ?? 'neutral'].color }}
    >
      {props.icon}
    </span>
    <h2 id={props.id} class="min-w-0 text-section" style={{ color: 'var(--color-text-primary)' }}>
      {props.children}
    </h2>
    <Show when={props.count !== undefined}>
      <span class="flex-none text-small font-semibold" style={{ color: 'var(--color-text-secondary)' }}>
        {props.count}
      </span>
    </Show>
    <Show when={props.actions}>
      <div class="ml-auto flex flex-none items-center gap-2">{props.actions}</div>
    </Show>
  </div>
);

export default SectionHeading;
