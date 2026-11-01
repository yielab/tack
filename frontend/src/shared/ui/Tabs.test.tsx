import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import Tabs from './Tabs';
import { flush } from '../../test/flush';

const disposers: Array<() => void> = [];
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

const TABS = ['one', 'two', 'three', 'four'].map((id) => ({ id, label: id }));

function mount(active = 'one', onChange: (id: string) => void = () => {}) {
  const c = document.createElement('div');
  document.body.appendChild(c);
  disposers.push(render(() => <Tabs tabs={TABS} active={active} onChange={onChange} />, c));
  return c.querySelector<HTMLElement>('[role="tablist"]')!;
}

const size = (el: HTMLElement, client: number, scroll: number) => {
  Object.defineProperty(el, 'clientWidth', { configurable: true, value: client });
  Object.defineProperty(el, 'scrollWidth', { configurable: true, value: scroll });
};

describe('Tabs', () => {
  it('shows no fade while every tab fits', () => {
    const list = mount();
    expect(list.getAttribute('data-fade')).toBeNull();
  });

  it('fades the edge that has more tabs behind it, and both once scrolled into the middle', async () => {
    const list = mount();
    size(list, 100, 300);
    list.dispatchEvent(new Event('scroll'));
    await flush();
    expect(list.getAttribute('data-fade')).toBe('end');
    list.scrollLeft = 50;
    list.dispatchEvent(new Event('scroll'));
    await flush();
    expect(list.getAttribute('data-fade')).toBe('both');
    list.scrollLeft = 200;
    list.dispatchEvent(new Event('scroll'));
    await flush();
    expect(list.getAttribute('data-fade')).toBe('start');
  });

  it('keeps the arrow keys moving between tabs, and the fade is not a tab stop', () => {
    const picked: string[] = [];
    const list = mount('one', (id) => picked.push(id));
    list.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    expect(picked).toEqual(['two']);
    expect(list.querySelectorAll('[tabindex="0"]')).toHaveLength(1);
  });
});
