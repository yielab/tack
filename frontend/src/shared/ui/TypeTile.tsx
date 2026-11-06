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

export interface TypeTileProps {
  /** A key of the project's type catalog. */
  type: string;
  /** Overrides the catalog's name. */
  label?: string;
  class?: string;
  style?: JSX.CSSProperties;
}

/**
 * A type's name in its catalog colour. A type that holds work is drawn as a stacked tile (a second
 * and third sheet showing beneath it) and a type that is work as a flat one, so the level reads
 * before the word. A key the catalog lacks reads as its capitalised self, neutral and flat.
 */
const TypeTile: Component<TypeTileProps> = (props) => {
  const types = useItemTypes();
  const tone = () => typeTone(types.color(props.type));
  const text = () => props.label ?? types.name(props.type);
  const holdsWork = () => types.get(props.type)?.level === 'group';
  return (
    <span
      data-testid="type-tile"
      data-holds-work={holdsWork() ? 'true' : 'false'}
      title={holdsWork() ? `${text()} holds work` : undefined}
      class={clsx('inline-flex items-center whitespace-nowrap font-bold', props.class)}
      style={{
        'background-color': tone().bg,
        color: tone().fg,
        'font-size': '0.65625rem',
        'letter-spacing': '.01em',
        padding: '2px 8px',
        'border-radius': holdsWork() ? 'var(--radius-sm)' : 'var(--radius-pill)',
        // The sheets beneath the stacked tile: the tile's own ink at two strengths.
        'box-shadow': holdsWork()
          ? `2px 2px 0 -0.5px ${tone().bg}, 2px 2px 0 0.5px color-mix(in srgb, ${tone().fg} 45%, transparent), 4px 4px 0 -0.5px ${tone().bg}, 4px 4px 0 0.5px color-mix(in srgb, ${tone().fg} 25%, transparent)`
          : undefined,
        'margin-right': holdsWork() ? '4px' : undefined,
        'margin-bottom': holdsWork() ? '4px' : undefined,
        ...props.style,
      }}
    >
      {text()}
    </span>
  );
};

export default TypeTile;
