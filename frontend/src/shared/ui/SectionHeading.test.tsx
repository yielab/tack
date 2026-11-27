import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import SectionHeading from './SectionHeading';
import { IconList } from './icons';

const disposers: Array<() => void> = [];
function mount(comp: () => unknown) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(comp as never, container);
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

describe('SectionHeading', () => {
  it('is an h2 at the section size, with the icon in a 28px rounded-md square beside it', () => {
    const c = mount(() => (
      <SectionHeading icon={<IconList size={16} />} id="needs">
        Needs you
      </SectionHeading>
    ));
    const h = c.querySelector('h2')!;
    expect(h.textContent).toBe('Needs you');
    expect(h.id).toBe('needs');
    expect(h.classList.contains('text-section')).toBe(true);
    const square = c.querySelector('[data-testid="section-icon"]')!;
    expect(square.getAttribute('aria-hidden')).toBe('true');
    expect(square.classList.contains('h-7')).toBe(true);
    expect(square.classList.contains('w-7')).toBe(true);
    expect(square.classList.contains('rounded-md')).toBe(true);
    expect(square.querySelector('svg')).not.toBeNull();
  });

  it.each([
    ['neutral', 'bg-subtle'],
    ['done', 'success-100'],
    ['agent', 'accent-soft'],
    ['people', 'info-100'],
    ['needs', 'warning-100'],
  ] as const)('tone %s tints the square from its own token', (tone, token) => {
    const c = mount(() => (
      <SectionHeading icon={<IconList size={16} />} tone={tone}>
        Work
      </SectionHeading>
    ));
    const square = c.querySelector<HTMLElement>('[data-testid="section-icon"]')!;
    expect(square.style.backgroundColor).toContain(token);
  });

  it('defaults to the neutral tone', () => {
    const c = mount(() => <SectionHeading icon={<IconList size={16} />}>Work</SectionHeading>);
    expect(c.querySelector<HTMLElement>('[data-testid="section-icon"]')!.style.backgroundColor).toContain('bg-subtle');
  });

  it('shows the count, including zero, and none when it is not given', () => {
    const zero = mount(() => (
      <SectionHeading icon={<IconList size={16} />} count={0}>
        Done
      </SectionHeading>
    ));
    expect(zero.textContent).toBe('Done0');
    const none = mount(() => <SectionHeading icon={<IconList size={16} />}>Done</SectionHeading>);
    expect(none.textContent).toBe('Done');
  });

  it('puts the actions at the end of the row, and no slot when there are none', () => {
    const c = mount(() => (
      <SectionHeading icon={<IconList size={16} />} actions={<button type="button">Add</button>}>
        Checks
      </SectionHeading>
    ));
    const slot = c.querySelector('button')!.parentElement!;
    expect(slot.classList.contains('ml-auto')).toBe(true);
    const bare = mount(() => <SectionHeading icon={<IconList size={16} />}>Checks</SectionHeading>);
    expect(bare.querySelector('.ml-auto')).toBeNull();
  });
});
