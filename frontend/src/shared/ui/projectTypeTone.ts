import type { JSX } from 'solid-js';

/** Token-based (soft fill, ink text) pair per project type, so a type reads by
 *  hue at a glance. Tokens only — the pair re-resolves per palette and mode.
 *  The palette has fewer hues than there are types, so related types share one. */
export function projectTypeTone(type: string): { bg: string; fg: string } {
  switch (type) {
    case 'software':
    case 'mobile':
    case 'homework':
      return { bg: 'var(--color-accent-soft)', fg: 'var(--color-accent-ink)' };
    case 'web':
    case 'personal':
    case 'legal':
      return { bg: 'var(--color-accent2-soft)', fg: 'var(--color-accent2-ink)' };
    case 'construction':
    case 'event':
      return { bg: 'var(--color-warning-100)', fg: 'var(--color-warning-600)' };
    case 'research':
      return { bg: 'var(--color-success-100)', fg: 'var(--color-success-600)' };
    default:
      return { bg: 'var(--color-chip)', fg: 'var(--color-text-secondary)' };
  }
}

/** The small type pill (11px, 600, soft fill + ink). */
export function projectTypePillStyle(type: string): JSX.CSSProperties {
  const tone = projectTypeTone(type);
  return {
    'background-color': tone.bg,
    color: tone.fg,
    'font-size': '11px',
    'font-weight': 600,
    padding: '3px 10px',
    'border-radius': 'var(--radius-pill)',
    'white-space': 'nowrap',
  };
}
