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

/** A priority as a solid fill under `--color-text-inverse` text — Calendar's
 *  chips, Timeline's bars. The same colors as priorityColor, except medium
 *  takes accent2's ink step: Harbor's coral accent2 carries inverse text at
 *  only 3.3:1, its ink step clears AA, and in every other palette the ink is
 *  accent2 itself. */
export function priorityFill(p: Priority): string {
  return p === 'medium' ? 'var(--color-accent2-ink)' : priorityColor(p);
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
      gap: '6px',
      'font-size': '0.75rem',
      'font-weight': 500,
      color: 'var(--color-text-secondary)',
    }}
  >
    <span
      style={{
        width: '8px',
        height: '8px',
        'border-radius': '3px',
        background: priorityColor(props.priority),
        'flex-shrink': 0,
      }}
    />
    {props.showLabel && (props.label ?? priorityLabel(props.priority))}
  </span>
);

export default PriorityDot;
