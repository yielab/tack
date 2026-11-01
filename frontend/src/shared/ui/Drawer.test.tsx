import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import Drawer from './Drawer';
import { truncateMiddle } from './truncateMiddle';

const disposers: Array<() => void> = [];

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

describe('Drawer', () => {
  // jsdom has no layout, so this cannot measure a scroll width. It checks the classes that
  // guarantee there is none: the body and panel may shrink and clip sideways, and the id is
  // shortened on screen while its full value stays in the title attribute.
  it('keeps a 200-character unbroken id from widening the drawer (checks the clip and shrink classes and the shortened text, not a measured width)', () => {
    const id = 'a'.repeat(200);
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <Drawer isOpen onClose={() => {}} title="Run">
          <dl>
            <dd>
              <span title={id}>{truncateMiddle(id)}</span>
            </dd>
          </dl>
        </Drawer>
      ),
      container,
    );
    disposers.push(() => { dispose(); container.remove(); });

    const panel = document.querySelector<HTMLElement>('[role="dialog"]')!;
    expect(panel.classList.contains('min-w-0')).toBe(true);

    const body = panel.querySelector<HTMLElement>('.overflow-x-clip')!;
    expect(body.classList.contains('overflow-y-auto')).toBe(true);
    expect(body.classList.contains('min-w-0')).toBe(true);
    expect(body.classList.contains('[&_*]:min-w-0')).toBe(true);
    expect(body.classList.contains('wrap-anywhere')).toBe(true);

    const shown = panel.querySelector<HTMLElement>(`[title="${id}"]`)!;
    expect(shown.textContent).toBe(`${'a'.repeat(8)}…${'a'.repeat(4)}`);
    expect(shown.textContent!.length).toBeLessThan(id.length);
  });
});
