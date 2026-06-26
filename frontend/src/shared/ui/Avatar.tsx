import { For, type Component, type JSX } from 'solid-js';
import clsx from 'clsx';

export type AvatarSize = 'sm' | 'md';

const PX: Record<AvatarSize, number> = { sm: 22, md: 26 };
const FONT: Record<AvatarSize, string> = { sm: '10px', md: '11px' };

/** Deterministic hue (0–359) from a name, so the same person is always the
 *  same color without storing one. Stable across reloads and machines. */
export function hueFromName(name: string): number {
  let h = 0;
  for (let i = 0; i < name.length; i++) {
    h = (h * 31 + name.charCodeAt(i)) % 360;
  }
  return h;
}

/** Up to two uppercase initials from a display name. */
export function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

export interface AvatarProps {
  /** Display name (drives the color) or pre-computed initials. */
  name: string;
  size?: AvatarSize;
  /** Render an outline ring (used inside stacks over a surface). */
  ring?: boolean;
  title?: string;
  style?: JSX.CSSProperties;
}

const Avatar: Component<AvatarProps> = (props) => {
  const size = () => props.size ?? 'md';
  const px = () => PX[size()];
  return (
    <span
      title={props.title ?? props.name}
      style={{
        width: `${px()}px`,
        height: `${px()}px`,
        'border-radius': '99px',
        background: `hsl(${hueFromName(props.name)} 45% 50%)`,
        color: '#fff',
        display: 'inline-flex',
        'align-items': 'center',
        'justify-content': 'center',
        'font-size': FONT[size()],
        'font-weight': 700,
        'flex-shrink': 0,
        ...(props.ring ? { border: '2px solid var(--color-bg-base)' } : {}),
        ...props.style,
      }}
    >
      {initialsOf(props.name)}
    </span>
  );
};

export interface AvatarStackProps {
  names: string[];
  max?: number;
  size?: AvatarSize;
  class?: string;
}

/** Overlapping avatars with a `+N` overflow chip. */
export const AvatarStack: Component<AvatarStackProps> = (props) => {
  const max = () => props.max ?? 4;
  const size = () => props.size ?? 'md';
  const shown = () => props.names.slice(0, max());
  const overflow = () => Math.max(0, props.names.length - max());
  const px = () => PX[size()];
  return (
    <div class={clsx('flex items-center', props.class)}>
      <For each={shown()}>
        {(name, i) => (
          <Avatar name={name} size={size()} ring style={i() === 0 ? {} : { 'margin-left': '-8px' }} />
        )}
      </For>
      {overflow() > 0 && (
        <span
          style={{
            width: `${px()}px`,
            height: `${px()}px`,
            'border-radius': '99px',
            background: 'var(--color-chip)',
            color: 'var(--color-text-secondary)',
            display: 'inline-flex',
            'align-items': 'center',
            'justify-content': 'center',
            'font-size': FONT[size()],
            'font-weight': 700,
            border: '2px solid var(--color-bg-base)',
            'margin-left': '-8px',
          }}
        >
          +{overflow()}
        </span>
      )}
    </div>
  );
};

export default Avatar;
