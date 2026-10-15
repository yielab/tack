import type { JSX } from 'solid-js';

export const SECTION = 'flex flex-col gap-2.5 rounded-[26px] bg-panel p-[18px]';

export const Heading = (props: { children: JSX.Element }) => (
  <h3 class="text-[19px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
    {props.children}
  </h3>
);

export const Note = (props: { children: JSX.Element; testid?: string }) => (
  <p data-testid={props.testid} class="text-[12.5px]" style={{ color: 'var(--color-text-secondary)' }}>
    {props.children}
  </p>
);

export const inputClass = 'w-full rounded-full border px-3.5 py-2 text-sm';
export const inputStyle = {
  'background-color': 'var(--color-bg-base)',
  color: 'var(--color-text-primary)',
  'border-color': 'var(--color-border-medium)',
} as const;

/** What a profile's `tool_policy.docket.team` names, or null. */
export function teamOf(profile: { tool_policy?: unknown } | undefined): string | null {
  const policy = profile?.tool_policy as { docket?: { team?: unknown } } | null | undefined;
  const team = policy?.docket?.team;
  return typeof team === 'string' && team ? team : null;
}
