import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { flush } from '../../test/flush';
import type { ParentOption } from '../types';
import ParentPicker from './ParentPicker';

const OPTIONS: ParentOption[] = [
  { id: 'a', title: 'Checkout', type: 'epic' },
  { id: 'b', title: 'Billing', type: 'epic' },
];

const disposers: Array<() => void> = [];
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

async function mount(extra: { current?: string | null; respond?: () => Response } = {}) {
  const urls: string[] = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    urls.push(String(input));
    return extra.respond?.() ?? new Response(JSON.stringify(OPTIONS), { status: 200, headers: { 'Content-Type': 'application/json' } });
  }) as typeof fetch);
  const onPick = vi.fn();
  const container = document.createElement('div');
  document.body.appendChild(container);
  disposers.push(render(() => <ParentPicker projectId="p1" of={{ item: 'x' }} current={extra.current} onPick={onPick} />, container));
  await flush();
  await flush();
  return { container, onPick, urls };
}

const key = (el: Element, k: string) => el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));
const names = (c: Element) => [...c.querySelectorAll('[role="option"]')].map((o) => o.textContent!.replace(/\s+/g, ' ').trim());

describe('ParentPicker', () => {
  it('asks_the_server_which_parents_it_admits_and_offers_exactly_those_and_no_parent', async () => {
    const { container, urls } = await mount();
    expect(urls[0]).toBe('/api/projects/p1/parents?item=x');
    expect(names(container)).toEqual(['No parent', 'EpicCheckout', 'EpicBilling']);
    const combo = container.querySelector('[role="combobox"]')!;
    expect(combo.getAttribute('aria-label')).toBe('Move under');
    expect(container.querySelector('[role="listbox"]')).not.toBeNull();
  });

  it('moves_with_the_arrow_keys_and_picks_with_enter', async () => {
    const { container, onPick } = await mount();
    const combo = container.querySelector('input')!;
    key(combo, 'ArrowDown');
    key(combo, 'ArrowDown');
    await flush();
    expect(combo.getAttribute('aria-activedescendant')).toBe(container.querySelectorAll('[role="option"]')[2].id);
    key(combo, 'Enter');
    expect(onPick).toHaveBeenCalledWith(OPTIONS[1]);
  });

  it('narrows_by_what_is_typed_and_picks_no_parent_as_null', async () => {
    const { container, onPick } = await mount();
    const combo = container.querySelector('input')!;
    combo.value = 'bill';
    combo.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await flush();
    expect(names(container)).toEqual(['EpicBilling']);
    combo.value = 'no p';
    combo.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await flush();
    (container.querySelector('[role="option"]') as HTMLElement).click();
    expect(onPick).toHaveBeenCalledWith(null);
  });

  it('marks_the_current_parent_and_does_not_offer_it_again', async () => {
    const { container, onPick } = await mount({ current: 'a' });
    const current = container.querySelectorAll('[role="option"]')[1] as HTMLElement;
    expect(current.getAttribute('aria-disabled')).toBe('true');
    expect(current.textContent).toContain('Current');
    current.click();
    expect(onPick).not.toHaveBeenCalled();
    key(container.querySelector('input')!, 'Enter'); // the highlight skipped it
    expect(onPick).toHaveBeenCalledWith(null);
  });

  it('says_when_nothing_can_hold_the_item_and_when_the_places_did_not_load', async () => {
    const empty = await mount({ current: null, respond: () => new Response('[]', { status: 200, headers: { 'Content-Type': 'application/json' } }) });
    expect(names(empty.container)).toEqual(['No parentCurrent']);
    const failed = await mount({ respond: () => new Response('{}', { status: 500 }) });
    expect(failed.container.querySelector('[role="alert"]')!.textContent).toContain('did not load');
    expect(failed.container.querySelector('[role="alert"] button')!.textContent).toBe('Try again');
  });
});
