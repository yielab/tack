import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import DetailsFold, { readDetailsOpen } from './DetailsFold';

const disposers: Array<() => void> = [];

function mount(glance = 'High · Ana') {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <DetailsFold glance={glance}><p data-testid="inside">Priority</p></DetailsFold>, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

const toggle = (c: Element) => c.querySelector<HTMLButtonElement>('button[aria-expanded]')!;

beforeEach(() => localStorage.clear());
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

describe('DetailsFold', () => {
  it('starts shut, says the values worth a glance, and keeps the properties out until it opens', () => {
    const c = mount();
    expect(toggle(c).getAttribute('aria-expanded')).toBe('false');
    expect(toggle(c).textContent).toContain('Details');
    expect(toggle(c).textContent).toContain('High · Ana');
    expect(c.querySelector('[data-testid="inside"]')).toBeNull();
  });

  it('opens on a press, and the next screen opens it again', () => {
    const first = mount();
    toggle(first).click();
    expect(toggle(first).getAttribute('aria-expanded')).toBe('true');
    expect(first.querySelector('[data-testid="inside"]')).not.toBeNull();
    // The glance is for a shut fold only.
    expect(toggle(first).textContent).not.toContain('High · Ana');
    expect(localStorage.getItem('tack_item_details_open')).toBe('1');

    const second = mount();
    expect(toggle(second).getAttribute('aria-expanded')).toBe('true');
    expect(second.querySelector('[data-testid="inside"]')).not.toBeNull();
  });

  it('remembers that it was shut again', () => {
    localStorage.setItem('tack_item_details_open', '1');
    const c = mount();
    toggle(c).click();
    expect(localStorage.getItem('tack_item_details_open')).toBe('0');
    expect(readDetailsOpen()).toBe(false);
  });

  it('works without storage: shut at first, and it still opens and shuts', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    const c = mount();
    expect(toggle(c).getAttribute('aria-expanded')).toBe('false');
    toggle(c).click();
    expect(toggle(c).getAttribute('aria-expanded')).toBe('true');
    toggle(c).click();
    expect(toggle(c).getAttribute('aria-expanded')).toBe('false');
  });
});
