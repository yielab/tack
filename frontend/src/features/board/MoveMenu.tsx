import { createEffect, createSignal, createUniqueId, For, onCleanup, Show, type Component } from 'solid-js';
import { Portal } from 'solid-js/web';
import { IconChevronDown } from '../../shared/ui/icons';

const ITEM_HEIGHT = 32;

/** A card's "Move to…" button and the menu of the other columns it opens: the
 *  keyboard's way to do what dragging does. The menu is a portal, so a column's
 *  own scrolling never clips it. Arrow keys move, Enter or Space picks, Escape
 *  closes and returns to the button. Announcing the move is the caller's: the
 *  board's move action raises the toast the page's live region reads out. */
const MoveMenu: Component<{
  /** The statuses to offer: every column but the card's own. */
  statuses: string[];
  onMove: (status: string) => void;
}> = (props) => {
  const menuId = createUniqueId();
  const [open, setOpen] = createSignal(false);
  const [anchor, setAnchor] = createSignal<{ right: number; top?: number; bottom?: number }>({ right: 0 });
  let button!: HTMLButtonElement;
  let menu: HTMLDivElement | undefined;

  const items = () => [...(menu?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];

  const openMenu = () => {
    const rect = button.getBoundingClientRect();
    const right = window.innerWidth - rect.right;
    // Opens downward, or upward when the menu would run past the window.
    const flips = rect.bottom + 8 + props.statuses.length * ITEM_HEIGHT > window.innerHeight;
    setAnchor(flips ? { right, bottom: window.innerHeight - rect.top + 4 } : { right, top: rect.bottom + 4 });
    setOpen(true);
    queueMicrotask(() => items()[0]?.focus());
  };

  const close = (returnFocus: boolean) => {
    setOpen(false);
    if (returnFocus) button.focus();
  };

  const pick = (status: string) => {
    close(true);
    props.onMove(status);
  };

  const step = (by: number | 'first' | 'last') => {
    const list = items();
    if (list.length === 0) return;
    const at = list.indexOf(document.activeElement as HTMLElement);
    const next = by === 'first' ? 0 : by === 'last' ? list.length - 1 : (at + by + list.length) % list.length;
    list[next].focus();
  };

  const onMenuKeyDown = (e: KeyboardEvent) => {
    const move: Record<string, number | 'first' | 'last'> = { ArrowDown: 1, ArrowUp: -1, Home: 'first', End: 'last' };
    if (e.key in move) { e.preventDefault(); step(move[e.key]); }
    else if (e.key === 'Escape' || e.key === 'Tab') { e.preventDefault(); close(true); }
  };

  // While open, a press outside, or a scroll or resize that moves the button,
  // closes it: the menu is fixed to where the button was.
  createEffect(() => {
    if (!open()) return;
    const placed = button.getBoundingClientRect();
    const outside = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!menu?.contains(target) && !button.contains(target)) setOpen(false);
    };
    const away = () => {
      const now = button.getBoundingClientRect();
      if (now.top !== placed.top || now.right !== placed.right) setOpen(false);
    };
    document.addEventListener('pointerdown', outside, true);
    window.addEventListener('scroll', away, true);
    window.addEventListener('resize', away);
    onCleanup(() => {
      document.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('scroll', away, true);
      window.removeEventListener('resize', away);
    });
  });

  return (
    <>
      <button
        ref={button}
        type="button"
        aria-label="Move to…"
        title="Move to…"
        aria-haspopup="menu"
        aria-expanded={open()}
        aria-controls={open() ? menuId : undefined}
        // Hidden until the card is hovered or holds focus, but never for a
        // touch screen, which has no hover, nor while its menu is open.
        class="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 [@media(hover:none)]:opacity-100"
        style={{
          width: '24px', height: '24px', padding: 0, border: 'none', 'border-radius': '50%',
          background: 'var(--color-bg-panel)', color: 'var(--color-text-secondary)', cursor: 'pointer',
          display: 'grid', 'place-items': 'center', 'flex-shrink': 0,
          opacity: open() ? 1 : undefined,
        }}
        onClick={(e) => { e.stopPropagation(); if (open()) close(false); else openMenu(); }}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); e.stopPropagation(); openMenu(); }
        }}
      >
        <IconChevronDown size={13} />
      </button>
      <Show when={open()}>
        <Portal>
          <div
            ref={menu}
            id={menuId}
            role="menu"
            aria-label="Move to…"
            onKeyDown={onMenuKeyDown}
            style={{
              position: 'fixed', right: `${anchor().right}px`, top: anchor().top != null ? `${anchor().top}px` : undefined,
              bottom: anchor().bottom != null ? `${anchor().bottom}px` : undefined,
              'z-index': 70, 'min-width': '160px', padding: '4px',
              background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)',
              'border-radius': '14px', 'box-shadow': 'var(--shadow-lg)',
            }}
          >
            <For each={props.statuses}>
              {(status) => (
                <button
                  type="button"
                  role="menuitem"
                  tabindex={-1}
                  onClick={() => pick(status)}
                  class="hover:bg-[var(--color-bg-hover)] focus-visible:bg-[var(--color-bg-hover)]"
                  style={{
                    display: 'block', width: '100%', 'min-height': `${ITEM_HEIGHT}px`, padding: '0 12px',
                    border: 'none', 'border-radius': '10px', background: 'transparent', cursor: 'pointer',
                    'text-align': 'left', 'font-size': '0.8125rem', 'font-weight': 500, color: 'var(--color-text-primary)',
                  }}
                >
                  {status}
                </button>
              )}
            </For>
          </div>
        </Portal>
      </Show>
    </>
  );
};

export default MoveMenu;
