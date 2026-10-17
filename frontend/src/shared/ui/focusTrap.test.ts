import { describe, it, expect, afterEach } from 'vitest';
import { trapFocus } from './focusTrap';

afterEach(() => { document.body.innerHTML = ''; });

function tab(container: HTMLElement, shiftKey = false) {
  const e = new KeyboardEvent('keydown', { key: 'Tab', shiftKey, cancelable: true });
  trapFocus(container, e);
  return e;
}

function setup() {
  document.body.innerHTML = `
    <button id="outside">outside</button>
    <div id="dialog" role="dialog" aria-modal="true" tabindex="-1">
      <button id="first">first</button>
      <button disabled>disabled</button>
      <input id="last" />
    </div>`;
  return document.getElementById('dialog')!;
}

describe('trapFocus', () => {
  it('wraps Tab from the last control to the first', () => {
    const dialog = setup();
    document.getElementById('last')!.focus();
    const e = tab(dialog);
    expect(e.defaultPrevented).toBe(true);
    expect(document.activeElement?.id).toBe('first');
  });

  it('wraps Shift+Tab from the first control to the last', () => {
    const dialog = setup();
    document.getElementById('first')!.focus();
    tab(dialog, true);
    expect(document.activeElement?.id).toBe('last');
  });

  it('pulls focus back in when it sits outside', () => {
    const dialog = setup();
    document.getElementById('outside')!.focus();
    tab(dialog);
    expect(document.activeElement?.id).toBe('first');
  });

  it('leaves Tab between inner controls to the browser', () => {
    const dialog = setup();
    document.getElementById('first')!.focus();
    expect(tab(dialog).defaultPrevented).toBe(false);
  });

  it('stays out of a modal stacked on top of it', () => {
    const dialog = setup();
    const top = document.createElement('div');
    top.setAttribute('aria-modal', 'true');
    top.innerHTML = '<button id="top">top</button>';
    document.body.appendChild(top);
    document.getElementById('top')!.focus();
    expect(tab(dialog).defaultPrevented).toBe(false);
    expect(document.activeElement?.id).toBe('top');
  });
});
