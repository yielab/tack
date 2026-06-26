import { type Component } from 'solid-js';
import type { Priority } from '../types';

/** Token color for a priority. critical→danger, high→warning, medium→accent2,
 *  low/none→faint. */
export function priorityColor(p: Priority): string {
  switch (p) {
    case 'critical':
      return 'var(--color-danger-600)';
    case 'high':
      return 'var(--color-warning-600)';
    case 'medium':
      return 'var(--color-accent2)';
    case 'low':
    case 'none':
    default:
      return 'var(--color-text-tertiary)';
  }
}

export function priorityLabel(p: Priority): string {
  return p.charAt(0).toUpperCase() + p.slice(1);
}

export interface PriorityDotProps {
  priority: Priority;
  /** Show the textual label after the dot. */
  showLabel?: boolean;
  label?: string;
}

const PriorityDot: Component<PriorityDotProps> = (props) => (
  <span
    style={{
      display: 'inline-flex',
      'align-items': 'center',
      gap: '4px',
      'font-size': '10.5px',
      'font-weight': 600,
      color: 'var(--color-text-secondary)',
    }}
  >
    <span
      style={{
        width: '7px',
        height: '7px',
        'border-radius': '2px',
        background: priorityColor(props.priority),
        'flex-shrink': 0,
      }}
    />
    {props.showLabel && (props.label ?? priorityLabel(props.priority))}
  </span>
);

export default PriorityDot;
