import { type Component, type JSX } from 'solid-js';
import clsx from 'clsx';

export type BadgeTone =
  | 'neutral'
  | 'primary'
  | 'success'
  | 'warning'
  | 'danger'
  | 'info';

export interface BadgeProps {
  tone?: BadgeTone;
  class?: string;
  children: JSX.Element;
}

function toneStyle(tone: BadgeTone): JSX.CSSProperties {
  const map: Record<BadgeTone, [string, string]> = {
    neutral: ['var(--color-bg-subtle)', 'var(--color-text-secondary)'],
    primary: ['var(--color-primary-100)', 'var(--color-primary-700)'],
    success: ['var(--color-success-100)', 'var(--color-success-700)'],
    warning: ['var(--color-warning-100)', 'var(--color-warning-700)'],
    danger: ['var(--color-danger-100)', 'var(--color-danger-700)'],
    info: ['var(--color-info-100)', 'var(--color-info-700)'],
  };
  const [bg, fg] = map[tone];
  return { 'background-color': bg, color: fg };
}

const Badge: Component<BadgeProps> = (props) => (
  <span
    class={clsx(
      'inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium',
      props.class
    )}
    style={toneStyle(props.tone ?? 'neutral')}
  >
    {props.children}
  </span>
);

export default Badge;
