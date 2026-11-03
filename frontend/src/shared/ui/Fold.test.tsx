import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { createSignal } from 'solid-js';
import Fold, { readFoldOpen, type FoldProps } from './Fold';
import { flush } from '../../test/flush';

const disposers: Array<() => void> = [];

function mount(props: Partial<FoldProps> = {}) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <Fold kind="details" title="Details" summary="Medium · no GitHub issue" lazy {...props}>
        <p data-testid="inside">Priority</p>
      </Fold>
    ),
    container,
  );
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

describe('Fold', () => {
  it('has a real button with aria-expanded and aria-controls, a chevron and the one-line summary while shut', () => {
    const c = mount();
    const button = toggle(c);
    expect(button.tagName).toBe('BUTTON');
    expect(button.getAttribute('aria-expanded')).toBe('false');
    expect(c.querySelector(`#${CSS.escape(button.getAttribute('aria-controls')!)}`)).not.toBeNull();
    expect(button.querySelector('svg')).not.toBeNull();
    expect(button.textContent).toContain('Details');
    expect(button.textContent).toContain('Medium · no GitHub issue');
    // A lazy fold keeps its content out of the page until it first opens.
    expect(c.querySelector('[data-testid="inside"]')).toBeNull();
  });

  it('opens on a press, drops the summary, and the next screen opens the same kind again', () => {
    const first = mount();
    toggle(first).click();
    expect(toggle(first).getAttribute('aria-expanded')).toBe('true');
    expect(first.querySelector('[data-testid="inside"]')).not.toBeNull();
    expect(toggle(first).textContent).not.toContain('Medium');
    expect(localStorage.getItem('tack_fold:details')).toBe('1');
    expect(readFoldOpen('details')).toBe(true);

    const second = mount();
    expect(toggle(second).getAttribute('aria-expanded')).toBe('true');
    // Another kind keeps its own default.
    expect(toggle(mount({ kind: 'technical' })).getAttribute('aria-expanded')).toBe('false');
  });

  it("lets the person override the section's default, either way", () => {
    localStorage.setItem('tack_fold:activity', '0');
    expect(toggle(mount({ kind: 'activity', defaultOpen: true })).getAttribute('aria-expanded')).toBe('false');
    localStorage.setItem('tack_fold:files', '1');
    expect(toggle(mount({ kind: 'files', defaultOpen: false })).getAttribute('aria-expanded')).toBe('true');
    expect(toggle(mount({ kind: 'steps', defaultOpen: true })).getAttribute('aria-expanded')).toBe('true');
  });

  it('says in its header what needs a person while shut, and opens by itself when it appears', async () => {
    const [needs, setNeeds] = createSignal<string | null>(null);
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => <Fold kind="questions2" title="Questions" needs={needs()}><p>Which one?</p></Fold>,
      container,
    );
    disposers.push(() => { dispose(); container.remove(); });
    expect(toggle(container).getAttribute('aria-expanded')).toBe('false');
    setNeeds('1 needs you');
    await flush();
    expect(toggle(container).getAttribute('aria-expanded')).toBe('true');
    // Opening by itself is not the person's choice: nothing is remembered.
    expect(localStorage.getItem('tack_fold:questions2')).toBeNull();
    // Shut by hand with the need still there, the header says it in text.
    toggle(container).click();
    expect(toggle(container).textContent).toContain('Questions');
    expect(toggle(container).textContent).toContain('1 needs you');
  });

  it('starts open when something needs a person, whatever was saved', () => {
    localStorage.setItem('tack_fold:questions', '0');
    expect(toggle(mount({ kind: 'questions', needs: '1 needs you' })).getAttribute('aria-expanded')).toBe('true');
  });

  it('works without storage: the defaults apply and it still opens and shuts', () => {
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

  it('tells its owner each time it opens or shuts', () => {
    const seen: boolean[] = [];
    const c = mount({ onToggle: (o) => seen.push(o) });
    toggle(c).click();
    toggle(c).click();
    expect(seen).toEqual([true, false]);
  });
});
