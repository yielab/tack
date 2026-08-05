import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import DispatchOutcomeNote from './DispatchOutcomeNote';

const disposers: Array<() => void> = [];

function mount(decision: string, detail?: string | null, title?: string) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <DispatchOutcomeNote decision={decision} detail={detail} title={title} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

describe('DispatchOutcomeNote', () => {
  it('renders the decision label as visible text, not color alone', () => {
    const c = mount('waiting_approval');
    expect(c.textContent).toContain('Waiting on approval');
  });

  it('renders the optional title when given (sprint results list)', () => {
    const c = mount('dispatched', null, 'Fix login bug');
    expect(c.textContent).toContain('Fix login bug');
  });

  it('omits the title block entirely when not given (item-detail context)', () => {
    const c = mount('dispatched');
    // The title span carries `.truncate`; the Badge itself does not.
    expect(c.querySelector('.truncate')).toBeNull();
  });

  it('renders the detail text when given', () => {
    const c = mount('blocked', 'guardrail policy "prompt-injection": nope');
    expect(c.textContent).toContain('prompt-injection');
  });

  it('renders no detail text when none is given', () => {
    const c = mount('dispatched');
    expect(c.textContent?.trim()).toBe('Dispatched');
  });

  it('renders the sprint-only decisions (waiting_on_dependencies, would_dispatch, error) without throwing', () => {
    for (const decision of ['waiting_on_dependencies', 'would_dispatch', 'error']) {
      const c = mount(decision);
      expect(c.textContent).toBeTruthy();
    }
  });
});
