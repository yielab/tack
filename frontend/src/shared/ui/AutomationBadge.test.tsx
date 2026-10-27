import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AutomationBadge, { type AutomationBadgeProps } from './AutomationBadge';

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = '';
});

function show(props: AutomationBadgeProps): HTMLElement {
  const host = document.createElement('div');
  document.body.appendChild(host);
  dispose = render(() => <AutomationBadge {...props} />, host);
  return host;
}

describe('AutomationBadge', () => {
  it('says "Started by itself" for a run the project started', () => {
    expect(show({ startedBy: 'auto_start' }).textContent).toBe('Started by itself');
  });

  it('names the team', () => {
    expect(show({ team: 'saas-content' }).textContent).toBe('Team: saas-content');
  });

  it('says "Merged by itself" for an automatic merge', () => {
    expect(show({ mergedBy: 'auto' }).textContent).toBe('Merged by itself');
  });

  it('shows nothing for a person-started run, no team and a merge by button', () => {
    expect(show({ startedBy: null, team: null, mergedBy: 'button' }).textContent).toBe('');
  });
});
