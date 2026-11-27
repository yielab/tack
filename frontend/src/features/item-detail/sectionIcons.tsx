import type { Component } from 'solid-js';

const svg = (size: number) => ({
  width: size,
  height: size,
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  'stroke-width': 1.9,
  'stroke-linecap': 'round' as const,
  'stroke-linejoin': 'round' as const,
  'aria-hidden': 'true' as const,
});

/** Three lines of text: the description. */
export const IconDescription: Component<{ size?: number }> = (props) => (
  <svg {...svg(props.size ?? 17)}>
    <path d="M5 6h14" />
    <path d="M5 11h14" />
    <path d="M5 16h9" />
  </svg>
);

/** A tick: what is done when. */
export const IconCheck: Component<{ size?: number }> = (props) => (
  <svg {...svg(props.size ?? 17)}>
    <path d="M5 12.5l4.5 4.5L19 7" />
  </svg>
);

/** A small robot: what the agent reads. */
export const IconRobot: Component<{ size?: number }> = (props) => (
  <svg {...svg(props.size ?? 17)}>
    <rect x="4" y="7" width="16" height="12" rx="2" />
    <path d="M12 7V4" />
    <circle cx="9" cy="13" r="1" />
    <circle cx="15" cy="13" r="1" />
  </svg>
);
