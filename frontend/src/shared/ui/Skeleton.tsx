import { type Component, type JSX } from 'solid-js';
import clsx from 'clsx';

export interface SkeletonProps {
  width?: string;
  height?: string;
  rounded?: boolean;
  class?: string;
}

/** A single shimmering placeholder block. Token-driven. */
const Skeleton: Component<SkeletonProps> = (props) => {
  const style: JSX.CSSProperties = {
    'background-color': 'var(--color-bg-subtle)',
    width: props.width ?? '100%',
    height: props.height ?? '1rem',
  };
  return (
    <div
      class={clsx('animate-pulse', props.rounded ? 'rounded-full' : 'rounded', props.class)}
      style={style}
      aria-hidden="true"
    />
  );
};

export default Skeleton;
