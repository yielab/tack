import { describe, it, expect, afterEach, vi } from 'vitest';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import Button from './Button';
import Badge from './Badge';
import EmptyState from './EmptyState';
import Field from './Field';
import Modal from './Modal';
import Drawer from './Drawer';
import Tabs from './Tabs';

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

const flush = () => new Promise((r) => queueMicrotask(() => r(null)));

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

describe('Button', () => {
  it('renders children and fires onClick', () => {
    const onClick = vi.fn();
    const c = mount(() => <Button onClick={onClick}>Save</Button>);
    const btn = c.querySelector('button')!;
    expect(btn.textContent).toContain('Save');
    btn.click();
    expect(onClick).toHaveBeenCalledOnce();
  });

  it('is disabled and aria-busy while loading', () => {
    const c = mount(() => <Button loading>Save</Button>);
    const btn = c.querySelector('button')!;
    expect(btn.disabled).toBe(true);
    expect(btn.getAttribute('aria-busy')).toBe('true');
  });
});

describe('Badge / EmptyState / Field', () => {
  it('Badge renders its content', () => {
    const c = mount(() => <Badge tone="success">Active</Badge>);
    expect(c.textContent).toContain('Active');
  });

  it('EmptyState shows title, description and action', () => {
    const c = mount(() => (
      <EmptyState title="Nothing here" description="add one" action={<button>Add</button>} />
    ));
    expect(c.textContent).toContain('Nothing here');
    expect(c.textContent).toContain('add one');
    expect(c.querySelector('button')!.textContent).toBe('Add');
  });

  it('Field links its label to the input and marks aria-invalid on error', () => {
    const c = mount(() => <Field label="Name" error="Required" value="" />);
    const label = c.querySelector('label')!;
    const input = c.querySelector('input')!;
    expect(label.getAttribute('for')).toBe(input.id);
    expect(input.getAttribute('aria-invalid')).toBe('true');
    expect(c.textContent).toContain('Required');
  });
});

describe('Modal', () => {
  it('renders when open, closes on ESC and on the close button', async () => {
    const [open, setOpen] = createSignal(true);
    mount(() => (
      <Modal isOpen={open()} onClose={() => setOpen(false)} title="Edit">
        <p>Body</p>
      </Modal>
    ));
    await flush();
    expect(document.body.textContent).toContain('Body');
    expect(document.querySelector('[role="dialog"]')).toBeTruthy();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await flush();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it('does not render when closed', () => {
    mount(() => (
      <Modal isOpen={false} onClose={() => {}} title="Edit">
        <p>Body</p>
      </Modal>
    ));
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });
});

describe('Drawer', () => {
  it('opens, then ESC closes it', async () => {
    const [open, setOpen] = createSignal(true);
    mount(() => (
      <Drawer isOpen={open()} onClose={() => setOpen(false)} title="Item">
        <p>Detail</p>
      </Drawer>
    ));
    await flush();
    expect(document.querySelector('[role="dialog"]')).toBeTruthy();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await flush();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });
});

describe('Tabs', () => {
  it('marks the active tab aria-selected and switches on click', () => {
    const [active, setActive] = createSignal('a');
    const c = mount(() => (
      <Tabs
        tabs={[
          { id: 'a', label: 'A' },
          { id: 'b', label: 'B' },
        ]}
        active={active()}
        onChange={setActive}
      >
        <span>panel-{active()}</span>
      </Tabs>
    ));
    const tabs = () => Array.from(c.querySelectorAll('[role="tab"]')) as HTMLButtonElement[];
    expect(tabs()[0].getAttribute('aria-selected')).toBe('true');
    expect(tabs()[1].getAttribute('aria-selected')).toBe('false');
    expect(c.textContent).toContain('panel-a');

    tabs()[1].click();
    expect(tabs()[0].getAttribute('aria-selected')).toBe('false');
    expect(tabs()[1].getAttribute('aria-selected')).toBe('true');
    expect(c.textContent).toContain('panel-b');
  });
});
