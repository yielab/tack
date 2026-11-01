import { type Component, type JSX, Show, createUniqueId } from 'solid-js';

export type RunSectionId = 'needs' | 'checks' | 'changes' | 'steps' | 'files' | 'technical';

export interface RunSectionProps {
  id: RunSectionId;
  title: string;
  /** One sentence with its number, beside the title: what the section says before the detail does. */
  summary?: JSX.Element;
  /** A closed disclosure instead of an open section: the third level, for what only a bug report needs. */
  fold?: boolean;
  /** Given the heading, so a button elsewhere can move focus to it. */
  headingRef?: (el: HTMLHeadingElement) => void;
  children: JSX.Element;
}

/**
 * One section of a run, under a hairline and with no panel of its own: a title, the sentence that
 * summarises it, then the detail. The order the sections come in is the run's reading order, so
 * the section id is on the element for a test or a deep link to find it by.
 */
const RunSection: Component<RunSectionProps> = (props) => {
  const headingId = createUniqueId();
  const rule = { 'border-color': 'var(--color-border-light)' };
  const summary = () => (
    <Show when={props.summary}>
      <span class="text-xs font-normal" style={{ color: 'var(--color-text-secondary)' }}>{props.summary}</span>
    </Show>
  );
  return (
    <Show
      when={!props.fold}
      fallback={
        <details data-testid="run-section" data-section={props.id} class="min-w-0 border-t pt-4" style={rule}>
          <summary class="cursor-pointer text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            {props.title} {summary()}
          </summary>
          <div class="mt-3 min-w-0 space-y-4">{props.children}</div>
        </details>
      }
    >
      <section data-testid="run-section" data-section={props.id} aria-labelledby={headingId} class="min-w-0 space-y-3 border-t pt-4" style={rule}>
        <div class="flex flex-wrap items-baseline gap-x-3 gap-y-0.5">
          <h4 id={headingId} ref={props.headingRef} tabIndex={-1} class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            {props.title}
          </h4>
          {summary()}
        </div>
        {props.children}
      </section>
    </Show>
  );
};

export default RunSection;
