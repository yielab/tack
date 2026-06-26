import { type Component, type JSX } from 'solid-js';

/** Style for a WIP-limit chip. Turns danger when the count exceeds the limit. */
export function wipChipStyle(count: number, limit: number): JSX.CSSProperties {
  const exceeded = count > limit;
  return {
    'font-family': 'var(--font-mono)',
    'font-size': '10px',
    'font-weight': 500,
    padding: '1px 6px',
    'border-radius': '5px',
    'background-color': exceeded ? 'var(--color-danger-100)' : 'var(--color-chip)',
    color: exceeded ? 'var(--color-danger-600)' : 'var(--color-text-tertiary)',
  };
}

export interface WipChipProps {
  count: number;
  limit: number;
}

/** `count/limit` chip; the board passes a column's size and `wip_limit`. */
const WipChip: Component<WipChipProps> = (props) => (
  <span style={wipChipStyle(props.count, props.limit)}>
    {props.count}/{props.limit}
  </span>
);

export default WipChip;
