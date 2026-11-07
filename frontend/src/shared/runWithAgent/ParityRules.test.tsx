import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ParityRules from './ParityRules';
import type { HarnessCapability } from '../execution/types';

afterEach(() => {
  document.body.innerHTML = '';
});

function mount(reports: Partial<HarnessCapability>[]) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <ParityRules reports={reports} />, container);
  return container;
}

const text = (c: HTMLElement, rule: string) => c.querySelector(`[data-testid="parity-${rule}"]`)?.textContent;

describe('ParityRules', () => {
  it('says per rule who applies it, and why nobody does when nobody does', () => {
    const c = mount([{
      parity: [
        { rule: 'sandbox_or_no_run', label: 'Runs in a sandbox, or does not run', by: 'runner' },
        { rule: 'egress_by_allow_list', label: 'Reaches the network only through the allow-list', by: 'delegated', to: 'docket' },
        { rule: 'no_sandbox_means_ask', label: 'Without a sandbox, asks', by: 'missing', reason: 'GR1: it never asks' },
        { rule: 'steps_visible', label: 'Its steps are visible', by: 'missing', reason: 'by design: it prints no steps' },
        { rule: 'redaction', label: 'Secrets are removed', by: 'cli' },
      ],
    }]);
    expect(text(c, 'sandbox_or_no_run')).toBe('Runs in a sandbox, or does not run: This computer, from outside the agent');
    expect(text(c, 'egress_by_allow_list')).toContain('docket itself');
    expect(text(c, 'no_sandbox_means_ask')).toContain('Nobody yet: it never asks');
    expect(text(c, 'no_sandbox_means_ask')).not.toContain('GR1');
    expect(text(c, 'steps_visible')).toContain('Nobody, and it cannot be: it prints no steps');
    expect(text(c, 'redaction')).toContain("The agent's own program");
  });

  it('shows nothing when the runner does not report the rules', () => {
    expect(mount([{}]).querySelector('[data-testid="parity-rules"]')).toBeNull();
  });
});
