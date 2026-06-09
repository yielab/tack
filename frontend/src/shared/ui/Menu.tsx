import {
  createSignal,
  onCleanup,
  Show,
  splitProps,
  type Component,
  type JSX,
} from 'solid-js';
import clsx from 'clsx';

export interface MenuProps {
  /** Trigger label (used when `trigger` is not provided). */
  label?: string;
  /** Custom trigger element. */
  trigger?: JSX.Element;
  children: JSX.Element;
  class?: string;
}

/** Lightweight dropdown menu: click to open, click-outside / ESC to close. */
const Menu: Component<MenuProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  let container: HTMLDivElement | undefined;

  const onDocClick = (e: MouseEvent) => {
    if (container && !container.contains(e.target as Node)) setOpen(false);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') setOpen(false);
  };

  const bind = () => {
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onKey);
  };
  const unbind = () => {
    document.removeEventListener('mousedown', onDocClick);
    document.removeEventListener('keydown', onKey);
  };
  onCleanup(unbind);

  const toggle = () => {
    const next = !open();
    setOpen(next);
    if (next) bind();
    else unbind();
  };

  return (
    <div ref={container} class={clsx('relative inline-block', props.class)}>
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open() ? 'true' : 'false'}
        onClick={toggle}
        class="inline-flex items-center gap-1 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors focus:outline-none focus-visible:ring-2"
        style={{
          'background-color': 'var(--color-bg-base)',
          color: 'var(--color-text-primary)',
          border: '1px solid var(--color-border-medium)',
          '--tw-ring-color': 'var(--color-focus-ring)',
        }}
      >
        {props.trigger ?? props.label ?? 'Menu'}
      </button>
      <Show when={open()}>
        <div
          role="menu"
          onClick={() => setOpen(false)}
          class="absolute right-0 z-40 mt-1 min-w-[10rem] overflow-hidden rounded-lg border py-1 shadow-lg"
          style={{
            'background-color': 'var(--color-bg-elevated)',
            'border-color': 'var(--color-border-light)',
          }}
        >
          {props.children}
        </div>
      </Show>
    </div>
  );
};

export interface MenuItemProps
  extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: 'default' | 'danger';
}

export const MenuItem: Component<MenuItemProps> = (props) => {
  const [local, rest] = splitProps(props, ['tone', 'class', 'children', 'style']);
  return (
    <button
      {...rest}
      role="menuitem"
      type="button"
      class={clsx(
        'block w-full px-4 py-2 text-left text-sm transition-colors hover:opacity-80 focus:outline-none',
        local.class
      )}
      style={{
        color:
          local.tone === 'danger'
            ? 'var(--color-danger-600)'
            : 'var(--color-text-primary)',
        ...(typeof local.style === 'object' ? local.style : {}),
      }}
    >
      {local.children}
    </button>
  );
};

export default Menu;
