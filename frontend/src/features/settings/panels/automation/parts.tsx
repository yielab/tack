import type { JSX } from 'solid-js';

export const SECTION = 'flex flex-col gap-2.5 rounded-[26px] bg-panel p-[18px]';

/** The levels a project can merge at, with the sentence the radios show and the short form the ladder shows. */
export const LEVELS = [
  { value: 'L3', short: 'You merge', sentence: 'You merge every change yourself.' },
  { value: 'L4', short: 'Inside the domains', sentence: 'Tack merges changes that stay inside the domains below.' },
  { value: 'L5', short: 'Anything that passes', sentence: 'Tack merges any change that passes its checks.' },
] as const;

/** The public-key field of the Merge block; the ladder's "Add the public key" link goes to it. */
export const PUBLIC_KEY_ID = 'automation-public-key';

/** What a run's token limit is when its profile sets none and the project has not changed it. */
export const DEFAULT_PROVISIONAL_TOKENS = 2_000_000;

/** A number with a space between the thousands, the way the board's own messages write it. */
export const spaced = (n: number) => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ' ');

/** A block inside a section: a heading and its fields, set off from the one above by a hairline, not by another panel. */
export const BLOCK = 'flex min-w-0 flex-col gap-2.5 border-t pt-4';
export const BLOCK_STYLE = { 'border-color': 'var(--color-border-medium)' } as const;

export const Block = (props: { id?: string; testid?: string; children: JSX.Element }) => (
  <div id={props.id} data-testid={props.testid} class={BLOCK} style={BLOCK_STYLE}>
    {props.children}
  </div>
);

/** One of the four sections of the tab. The index beside them links to `id`. */
export const Section = (props: { id: string; title: string; lead: string; children: JSX.Element }) => (
  <section id={props.id} data-testid={props.id} aria-labelledby={`${props.id}-title`} class={`${SECTION} min-w-0 scroll-mt-4`}>
    <div class="flex flex-col gap-0.5">
      <h2 id={`${props.id}-title`} class="text-2xl leading-tight" style={{ color: 'var(--color-text-primary)' }}>
        {props.title}
      </h2>
      <Note>{props.lead}</Note>
    </div>
    {props.children}
  </section>
);

export const Heading = (props: { children: JSX.Element }) => (
  <h3 class="text-[1.1875rem] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
    {props.children}
  </h3>
);

export const Note = (props: { children: JSX.Element; testid?: string }) => (
  <p data-testid={props.testid} class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>
    {props.children}
  </p>
);

export const inputClass = 'w-full rounded-full border px-3.5 py-2 text-sm';
export const inputStyle = {
  'background-color': 'var(--color-bg-base)',
  color: 'var(--color-text-primary)',
  'border-color': 'var(--color-border-medium)',
} as const;

export { teamOf } from '../../../../shared/execution';
