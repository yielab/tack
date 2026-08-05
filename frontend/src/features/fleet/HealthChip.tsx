import { type Component, type JSX } from 'solid-js';
import { Badge } from '../../shared/ui';
import type { ControlPlaneHealth } from './api';
import { HEALTH_LABEL, HEALTH_TONE } from './format';

export interface HealthChipProps {
  health: ControlPlaneHealth;
}

const DOT_COLOR: Record<ControlPlaneHealth, string> = {
  healthy: 'var(--color-success-600)',
  degraded: 'var(--color-warning-600)',
  unreachable: 'var(--color-danger-600)',
  unknown: 'var(--color-text-tertiary)',
};

/**
 * Pod health chip — the fleet row's primary at-a-glance signal. Four
 * visually distinct states (healthy / degraded / unreachable / unknown),
 * each a token-driven color plus a text label so the distinction never
 * relies on color alone (WCAG 1.4.1 — also keeps this readable for anyone
 * with color-vision deficiency, not just axe-clean).
 */
const HealthChip: Component<HealthChipProps> = (props) => {
  const content = (): JSX.Element => (
    <>
      <span
        aria-hidden="true"
        style={{
          display: 'inline-block',
          width: '6px',
          height: '6px',
          'border-radius': '99px',
          background: DOT_COLOR[props.health],
          'margin-right': '5px',
        }}
      />
      {HEALTH_LABEL[props.health]}
    </>
  );

  return <Badge tone={HEALTH_TONE[props.health]}>{content()}</Badge>;
};

export default HealthChip;
