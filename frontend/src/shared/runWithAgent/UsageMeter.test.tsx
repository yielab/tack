import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import UsageMeter from './UsageMeter';

const disposers: Array<() => void> = [];
function mount(used: number, limit: number) {
  const c = document.createElement('div');
  document.body.appendChild(c);
  const dispose = render(() => <UsageMeter testId="m" used={used} limit={limit} />, c);
  disposers.push(() => {
    dispose();
    c.remove();
  });
  return c.querySelector<HTMLElement>('[data-testid="m"]')!;
}
afterEach(() => {
  while (disposers.length) disposers.pop()!();
});

describe('UsageMeter', () => {
  it('is a meter named by the sentence, filled to the share used', () => {
    const meter = mount(120_000, 300_000);
    expect(meter.getAttribute('role')).toBe('meter');
    expect(meter.getAttribute('aria-label')).toBe('Used 120 000 of 300 000 tokens');
    expect(meter.getAttribute('data-near-limit')).toBe('false');
    expect((meter.firstElementChild as HTMLElement).style.width).toBe('40%');
  });

  it('turns amber near the limit and never overfills', () => {
    const meter = mount(450_000, 300_000);
    expect(meter.getAttribute('data-near-limit')).toBe('true');
    expect(meter.getAttribute('aria-valuenow')).toBe('300000');
    expect((meter.firstElementChild as HTMLElement).style.width).toBe('100%');
  });
});
