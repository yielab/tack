import { type Component, type JSX, For, Show, createEffect, createSignal, createUniqueId, onCleanup } from 'solid-js';

export interface ItemMenuEntry {
  label: string;
  onSelect: () => void;
  /** Why the entry cannot be used now; it stays in the menu, greyed out, with this beside it. */
  disabledReason?: string;
  /** The option the item is in now: it says so in words, and the menu opens on it. */
  current?: boolean;
  /** A mark before the label (a status dot), decorative: the label carries the meaning. */
  leading?: JSX.Element;
}

/**
 * A small menu behind one round button, the "⋯" of the item's actions by default: the actions that are
 * not what to do next, or the options of one choice (the status). Arrow keys move, Enter picks,
 * Escape closes and returns to the button; a press outside closes it.
 */
const ItemMenu: Component<{
  entries: ItemMenuEntry[];
  /** The button's accessible name; "More actions" when absent. */
  buttonLabel?: string;
  /** The menu's accessible name; "Item actions" when absent. */
  menuLabel?: string;
  /** What the button shows; "⋯" when absent. */
  icon?: JSX.Element;
}> = (props) => {
  const menuId = createUniqueId();
  const [open, setOpen] = createSignal(false);
  let root: HTMLDivElement | undefined;
  let button!: HTMLButtonElement;
  let menu: HTMLDivElement | undefined;

  const items = () => [...(menu?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not([disabled])') ?? [])];
  const close = (returnFocus: boolean) => {
    setOpen(false);
    if (returnFocus) button.focus();
  };
  const toggle = () => {
    const next = !open();
    setOpen(next);
    if (next) queueMicrotask(() => (menu?.querySelector<HTMLButtonElement>('[aria-current="true"]') ?? items()[0])?.focus());
  };
  const step = (by: number | 'first' | 'last') => {
    const list = items();
    if (list.length === 0) return;
    const at = list.indexOf(document.activeElement as HTMLButtonElement);
    const next = by === 'first' ? 0 : by === 'last' ? list.length - 1 : (at + by + list.length) % list.length;
    list[next].focus();
  };
  const onMenuKeyDown = (e: KeyboardEvent) => {
    const move: Record<string, number | 'first' | 'last'> = { ArrowDown: 1, ArrowUp: -1, Home: 'first', End: 'last' };
    if (e.key in move) {
      e.preventDefault();
      step(move[e.key]);
    } else if (e.key === 'Escape' || e.key === 'Tab') {
      // A native listener, so stopping the event keeps the drawer, which listens on the document, from closing too.
      e.preventDefault();
      e.stopPropagation();
      close(true);
    }
  };

  createEffect(() => {
    if (!open()) return;
    const outside = (e: PointerEvent) => {
      if (root && !root.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', outside, true);
    onCleanup(() => document.removeEventListener('pointerdown', outside, true));
  });

  return (
    <div ref={root} class="relative inline-flex">
      <button
        ref={button}
        type="button"
        aria-label={props.buttonLabel ?? 'More actions'}
        aria-haspopup="menu"
        aria-expanded={open()}
        aria-controls={open() ? menuId : undefined}
        onClick={toggle}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown' && !open()) {
            e.preventDefault();
            toggle();
          }
        }}
        class="grid h-8 w-8 place-items-center rounded-full text-lg leading-none focus:outline-none focus-visible:ring-2"
        style={{
          color: 'var(--color-text-secondary)',
          border: '1px solid var(--color-border-strong)',
          '--tw-ring-color': 'var(--color-focus-ring)',
        }}
      >
        <span aria-hidden="true" class="inline-flex">{props.icon ?? '⋯'}</span>
      </button>
      <Show when={open()}>
        <div
          ref={menu}
          id={menuId}
          role="menu"
          aria-label={props.menuLabel ?? 'Item actions'}
          on:keydown={onMenuKeyDown}
          class="absolute right-0 top-full z-50 mt-1 min-w-56 max-w-xs rounded-item p-1"
          style={{ 'background-color': 'var(--color-bg-elevated)', border: '1px solid var(--color-border-medium)', 'box-shadow': 'var(--shadow-md)' }}
        >
          <For each={props.entries}>
            {(entry) => (
              <>
                <button
                  type="button"
                  role="menuitem"
                  disabled={!!entry.disabledReason}
                  aria-describedby={entry.disabledReason ? `${menuId}-why` : undefined}
                  aria-current={entry.current ? 'true' : undefined}
                  onClick={() => {
                    close(false);
                    entry.onSelect();
                  }}
                  class="flex w-full items-center gap-2 rounded-item px-3 py-2 text-left text-sm hover:brightness-95 focus:outline-none focus-visible:ring-2 disabled:cursor-not-allowed disabled:opacity-55"
                  style={{ color: 'var(--color-text-primary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
                >
                  {entry.leading}
                  <span class="min-w-0 flex-1">{entry.label}</span>
                  <Show when={entry.current}>
                    <span class="flex-none text-xs font-semibold" style={{ color: 'var(--color-text-secondary)' }}>Current</span>
                  </Show>
                </button>
                <Show when={entry.disabledReason}>
                  <p id={`${menuId}-why`} class="px-3 pb-1.5 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                    {entry.disabledReason}
                  </p>
                </Show>
              </>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

export default ItemMenu;
