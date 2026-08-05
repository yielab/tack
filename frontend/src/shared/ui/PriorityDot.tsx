import { type Component } from 'solid-js';
import type { Priority } from '../types';

/** Token color for a priority. critical→danger, high→warning, medium→accent2,
 *  low/none→faint (muted, but still `--color-text-secondary` rather than the
 *  fainter `-tertiary` tier: this value also gets used as a *solid fill*
 *  behind `--color-text-inverse` text — Calendar.tsx's event chips and
 *  Timeline.tsx's Gantt bars — and `-tertiary` fails WCAG AA (as low as
 *  3.84:1) against inverse text in all three dark-mode palettes; `-secondary`
 *  clears 4.5:1 with margin in all six palette×mode combinations). */
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
      return 'var(--color-text-secondary)';
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
