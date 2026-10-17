import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import DisplaySettings from './DisplaySettings';
import { applyDisplay, DEFAULT_DISPLAY, resetDisplay } from '../../shared/state/display';

beforeEach(() => {
  const store = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
    clear: () => store.clear(),
  });
});

afterEach(() => {
  resetDisplay();
  applyDisplay(DEFAULT_DISPLAY);
  vi.unstubAllGlobals();
  document.body.innerHTML = '';
  document.documentElement.removeAttribute('data-palette');
  document.documentElement.classList.remove('light', 'dark');
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <DisplaySettings cardClass="" titleClass="" />, container);
  return container;
}

const group = (c: HTMLElement, legend: string) =>
  Array.from(c.querySelectorAll('fieldset')).find((f) => f.querySelector('legend')?.textContent === legend)!;

const radio = (f: HTMLFieldSetElement, label: string) =>
  Array.from(f.querySelectorAll('label')).find((l) => l.textContent?.trim() === label)!.querySelector('input')!;

describe('DisplaySettings', () => {
  it('offers each preference as a named group of native radios', () => {
    const c = mount();
    for (const legend of ['Theme', 'Palette', 'Text size', 'Zoom']) {
      const f = group(c, legend);
      expect(f, legend).toBeTruthy();
      expect(f.querySelectorAll('input[type="radio"]').length).toBeGreaterThan(1);
    }
  });

  it('applies a text size to the root as soon as it is picked', () => {
    const c = mount();
    radio(group(c, 'Text size'), '150%').click();
    expect(document.documentElement.style.fontSize).toBe('150%');
    expect(radio(group(c, 'Text size'), '150%').checked).toBe(true);
  });

  it('moves the palette here from the sidebar', () => {
    const c = mount();
    radio(group(c, 'Palette'), 'Clay').click();
    expect(document.documentElement.getAttribute('data-palette')).toBe('clay');
  });

  it('shows zoom disabled in a browser tab, with the reason', () => {
    const c = mount();
    const zoom = group(c, 'Zoom');
    expect(zoom.disabled).toBe(true);
    expect(zoom.textContent).toContain('browser’s own zoom');
  });

  it('turns on the high-legibility font and reduced motion', () => {
    const c = mount();
    c.querySelector<HTMLInputElement>('#legible-font')!.click();
    c.querySelector<HTMLInputElement>('#reduce-motion')!.click();
    expect(document.documentElement.getAttribute('data-font')).toBe('legible');
    expect(document.documentElement.getAttribute('data-motion')).toBe('reduce');
  });
});
