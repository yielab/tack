import { type Component, type JSX } from 'solid-js';
import clsx from 'clsx';
import type { TypeColor } from '../types';
import { useItemTypes } from '../vocab/useItemTypes';

/** Token-based color pair (bg, fg) per catalog colour. */
export function typeTone(color: TypeColor): { bg: string; fg: string } {
  switch (color) {
    case 'accent':
      return { bg: 'var(--color-accent-soft)', fg: 'var(--color-accent-ink)' };
    case 'accent2':
      return { bg: 'var(--color-accent2-soft)', fg: 'var(--color-accent2-ink)' };
    case 'danger':
      return { bg: 'var(--color-danger-100)', fg: 'var(--color-danger-600)' };
    case 'warning':
      return { bg: 'var(--color-warning-100)', fg: 'var(--color-warning-600)' };
    case 'success':
      return { bg: 'var(--color-success-100)', fg: 'var(--color-success-600)' };
    default:
      return { bg: 'var(--color-chip)', fg: 'var(--color-text-secondary)' };
  }
}

export interface TypeBadgeProps {
  /** A key of the project's type catalog. */
  type: string;
  /** Overrides the catalog's name. */
  label?: string;
  class?: string;
  style?: JSX.CSSProperties;
}

/** A type's name in its catalog colour; a key the catalog lacks reads as its capitalised self, neutral. */
const TypeBadge: Component<TypeBadgeProps> = (props) => {
  const types = useItemTypes();
  const tone = () => typeTone(types.color(props.type));
  const text = () => props.label ?? types.name(props.type);
  return (
    <span
      class={clsx(
        'inline-flex items-center whitespace-nowrap rounded-full font-bold',
        props.class
      )}
      style={{
        'background-color': tone().bg,
        color: tone().fg,
        'font-size': '0.65625rem',
        'letter-spacing': '.01em',
        padding: '2px 8px',
        ...props.style,
      }}
    >
      {text()}
    </span>
  );
};

export default TypeBadge;
