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
  // Shares degraded's amber — see `format.ts#HEALTH_TONE`'s doc comment for
  // why the label, not the color, is what has to carry the distinction.
  unconfigured: 'var(--color-warning-600)',
};

/**
 * Pod health chip — the fleet row's primary at-a-glance signal. Five states
 * (healthy / degraded / unreachable / unknown / unconfigured), each a
 * token-driven color plus a text label so the distinction never relies on
 * color alone (WCAG 1.4.1 — also keeps this readable for anyone with
 * color-vision deficiency, not just axe-clean). `degraded` and
 * `unconfigured` deliberately share a dot color; their labels differ.
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
