import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { HistoryLine } from './AgentProfilesPanel';

const history = (n: number) => ({
  n,
  p50: 300_000,
  p90: 480_000,
  min: 180_000,
  max: 520_000,
  proposal: n >= 5 ? 720_000 : null,
  runs_needed: 5,
});

function mount(n: number, onApply = vi.fn()) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <HistoryLine history={history(n)} onApply={onApply} />, container);
  return { container, onApply };
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('a profile form\'s history line', () => {
  it('proposes a limit from 5 runs, and applies it only on a click', () => {
    const { container, onApply } = mount(7);
    expect(container.textContent).toContain('The last 7 runs used 180 000 to 520 000 tokens. Proposed limit: 720 000');
    expect(onApply).not.toHaveBeenCalled();
    container.querySelector('button')!.click();
    expect(onApply).toHaveBeenCalledWith(720_000);
  });

  it('says how many runs are still needed under 5', () => {
    const { container } = mount(3);
    expect(container.textContent).toContain('3 of 5 successful runs measured before a limit is proposed');
    expect(container.querySelector('button')).toBeNull();
  });
});
