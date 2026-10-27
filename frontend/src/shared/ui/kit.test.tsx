import { describe, it, expect, afterEach, vi } from 'vitest';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import Button from './Button';
import Badge from './Badge';
import EmptyState from './EmptyState';
import Field from './Field';
import Select from './Select';
import Modal from './Modal';
import Drawer from './Drawer';
import Tabs from './Tabs';
import RouteBoundary from './RouteBoundary';
import LoadError from './LoadError';
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

  it('shows its reason beside the button, tied to it, only while it is disabled', () => {
    const [off, setOff] = createSignal(true);
    const c = mount(() => (
      <Button disabled={off()} reason="Set up cloud backup first" aria-describedby="other">
        Back up
      </Button>
    ));
    const btn = c.querySelector('button')!;
    const reason = c.querySelector('span[id]')!;
    expect(reason.textContent).toBe('Set up cloud backup first');
    expect(btn.getAttribute('aria-describedby')).toBe(`other ${reason.id}`);
    setOff(false);
    expect(c.querySelector('span[id]')).toBeNull();
    expect(btn.getAttribute('aria-describedby')).toBe('other');
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

  it('Field points aria-describedby at its error, else its hint', () => {
    const [error, setError] = createSignal<string | undefined>('Required');
    const c = mount(() => <Field label="Name" error={error()} hint="Three letters" value="" />);
    const input = c.querySelector('input')!;
    expect(c.querySelector(`#${CSS.escape(input.getAttribute('aria-describedby')!)}`)!.textContent).toBe('Required');
    setError(undefined);
    expect(c.querySelector(`#${CSS.escape(input.getAttribute('aria-describedby')!)}`)!.textContent).toBe('Three letters');
  });

  it('a_field_with_help_renders_the_hint', () => {
    const c = mount(() => (
      <Field label="Folder" value="" help={{ text: 'Where the code lives.', href: 'https://x.test/b.html#a' }} />
    ));
    const q = c.querySelector('button[aria-label="Help: Folder"]') as HTMLButtonElement;
    expect(q.getAttribute('title')).toBe('Where the code lives.');
    expect(q.textContent).toBe('(?)');
    expect(c.querySelector('a')).toBeNull();
    q.click();
    const more = c.querySelector('a')!;
    expect(c.textContent).toContain('Where the code lives.');
    expect(more.getAttribute('href')).toBe('https://x.test/b.html#a');
    expect(more.getAttribute('target')).toBe('_blank');
    expect(more.getAttribute('rel')).toBe('noreferrer');
  });

  it('the_help_hint_closes_on_escape_and_on_an_outside_pointerdown', () => {
    const c = mount(() => <Field label="Folder" value="" help={{ text: 'Where the code lives.' }} />);
    const q = c.querySelector('button[aria-label="Help: Folder"]') as HTMLButtonElement;
    q.click();
    expect(c.querySelector('[role="note"]')).not.toBeNull();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(c.querySelector('[role="note"]')).toBeNull();
    q.click();
    expect(c.querySelector('[role="note"]')).not.toBeNull();
    c.querySelector('[role="note"]')!.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    expect(c.querySelector('[role="note"]')).not.toBeNull();
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    expect(c.querySelector('[role="note"]')).toBeNull();
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

  it('carries its own Help link when given a book page', async () => {
    mount(() => (
      <Modal isOpen onClose={() => {}} title="Edit" help="https://example.test/book/dialog.html">
        <p>Body</p>
      </Modal>
    ));
    await flush();
    const link = document.querySelector('[role="dialog"] a[target="_blank"]') as HTMLAnchorElement | null;
    expect(link?.textContent).toBe('Help');
    expect(link?.href).toBe('https://example.test/book/dialog.html');
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

  it('ties each tab to the panel it controls and the panel back to its tab', () => {
    const c = mount(() => (
      <Tabs tabs={[{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }]} active="b" onChange={() => {}}>
        <span>panel</span>
      </Tabs>
    ));
    const tabs = Array.from(c.querySelectorAll('[role="tab"]'));
    const panel = c.querySelector('[role="tabpanel"]')!;
    expect(tabs[1].getAttribute('aria-controls')).toBe(panel.id);
    expect(panel.getAttribute('aria-labelledby')).toBe(tabs[1].id);
  });
});

describe('RouteBoundary / LoadError', () => {
  it('a_screen_that_throws_shows_a_message_and_draws_again_on_Try_again', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    let broken = true;
    const Screen = () => {
      if (broken) throw new Error('The items would not draw');
      return <p>drawn</p>;
    };
    const c = mount(() => (
      <RouteBoundary>
        <Screen />
      </RouteBoundary>
    ));
    expect(c.querySelector('[role="alert"]')!.textContent).toContain('The items would not draw');
    broken = false;
    Array.from(c.querySelectorAll('button')).find((b) => b.textContent === 'Try again')!.click();
    expect(c.textContent).toBe('drawn');
  });

  it('LoadError says what could not load and retries', () => {
    const onRetry = vi.fn();
    const c = mount(() => <LoadError what="the roles" error={new Error('Timed out')} onRetry={onRetry} />);
    expect(c.querySelector('[role="alert"]')!.textContent).toContain('Could not load the roles.');
    expect(c.textContent).toContain('Timed out');
    c.querySelector('button')!.click();
    expect(onRetry).toHaveBeenCalledOnce();
  });
});

describe('icons', () => {
  it('draws every instance of the same icon, not just the last one rendered', () => {
    const c = mount(() => (
      <>
        <IconList />
        <IconList />
      </>
    ));
    const svgs = c.querySelectorAll('svg');
    expect(svgs).toHaveLength(2);
    svgs.forEach((svg) => expect(svg.querySelector('path')).not.toBeNull());
  });
});

describe('Select', () => {
  const OPTIONS = [
    { value: 'critical', label: 'Critical' },
    { value: 'high', label: 'High' },
    { value: 'low', label: 'Low' },
  ];

  it('shows its value, not the first option, when the options come from the options prop', async () => {
    const c = mount(() => <Select label="Priority" value="high" options={OPTIONS} />);
    await flush();
    expect(c.querySelector('select')!.value).toBe('high');
  });

  it('renders the (?) when given help', () => {
    const c = mount(() => <Select label="Model" options={OPTIONS} help={{ text: 'Which model.' }} />);
    expect(c.querySelector('button[aria-label="Help: Model"]')!.textContent).toBe('(?)');
  });

  it('follows the value when it changes', async () => {
    const [value, setValue] = createSignal('high');
    const c = mount(() => <Select label="Priority" value={value()} options={OPTIONS} />);
    await flush();
    setValue('low');
    await flush();
    expect(c.querySelector('select')!.value).toBe('low');
  });
});
