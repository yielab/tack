import type { JSX } from 'solid-js';

export const SECTION = 'flex flex-col gap-2.5 rounded-[26px] bg-panel p-[18px]';

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
