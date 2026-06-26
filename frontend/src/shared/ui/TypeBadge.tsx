import { type Component, type JSX } from 'solid-js';
import clsx from 'clsx';
import type { ItemType } from '../types';

/** Normalize an item type (string or `{ custom }`) to its key. */
export function typeKey(t: ItemType): string {
  return typeof t === 'string' ? t : t.custom;
}

/** Token-based color pair (bg, fg) per type key. Colored types — Epic (accent),
 *  Feature (secondary), Bug (danger) — read at a glance; the rest stay neutral. */
export function typeBadgeTone(key: string): { bg: string; fg: string } {
  switch (key) {
    case 'epic':
      return { bg: 'var(--color-accent-soft)', fg: 'var(--color-accent-ink)' };
    case 'feature':
      return { bg: 'var(--color-accent2-soft)', fg: 'var(--color-accent2)' };
    case 'bug':
      return { bg: 'var(--color-danger-100)', fg: 'var(--color-danger-600)' };
    default:
      return { bg: 'var(--color-chip)', fg: 'var(--color-text-secondary)' };
  }
}

export interface TypeBadgeProps {
  type: ItemType;
  /** Display label (e.g. from vocab). Falls back to the capitalized key. */
  label?: string;
  class?: string;
  style?: JSX.CSSProperties;
}

const TypeBadge: Component<TypeBadgeProps> = (props) => {
  const key = () => typeKey(props.type);
  const tone = () => typeBadgeTone(key());
  const text = () => props.label ?? key().charAt(0).toUpperCase() + key().slice(1);
  return (
    <span
      class={clsx(
        'inline-flex items-center rounded-md font-semibold',
        props.class
      )}
      style={{
        'background-color': tone().bg,
        color: tone().fg,
        'font-size': '10.5px',
        'letter-spacing': '.01em',
        padding: '2px 7px',
        ...props.style,
      }}
    >
      {text()}
    </span>
  );
};

export default TypeBadge;
