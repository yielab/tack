import { createEffect, createSignal, createUniqueId, For, onCleanup, Show, type Component } from 'solid-js';
import { Portal } from 'solid-js/web';
import { IconChevronDown } from '../../shared/ui/icons';

const ITEM_HEIGHT = 32;

export interface MoveChoice {
  key: string;
  label: string;
  /** Why the task cannot go there; the entry stays listed, off, with this under it. */
  reason?: string;
}

/**
 * A token's "Move to…" button and the menu of the lanes it opens: the keyboard's way to do what
 * dragging does. A lane that cannot take the task is listed off with its reason. The menu is a
 * portal, so a lane's own scrolling never clips it. Arrow keys move, Enter or Space picks, Escape
 * closes and returns to the button.
 */
const MoveMenu: Component<{
  title: string;
  choices: MoveChoice[];
  onPick: (key: string) => void;
  /** Why no lane can take the task at all; the button is then off. */
  offReason?: string | null;
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
    const flips = rect.bottom + 8 + props.choices.length * (ITEM_HEIGHT + 16) > window.innerHeight;
    setAnchor(flips ? { right, bottom: window.innerHeight - rect.top + 4 } : { right, top: rect.bottom + 4 });
    setOpen(true);
    queueMicrotask(() => items().find((el) => el.getAttribute('aria-disabled') !== 'true')?.focus());
  };

  const close = (returnFocus: boolean) => {
    setOpen(false);
    if (returnFocus) button.focus();
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
    if (e.key in move) {
      e.preventDefault();
      step(move[e.key]);
    } else if (e.key === 'Escape' || e.key === 'Tab') {
      e.preventDefault();
      close(true);
    }
  };

  // While open, a press outside, or a scroll or resize that moves the button, closes it.
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
        data-testid="move-to"
        aria-label={`Move ${props.title} to…`}
        title={props.offReason ?? 'Move to…'}
        aria-disabled={props.offReason ? 'true' : undefined}
        aria-haspopup="menu"
        aria-expanded={open()}
        aria-controls={open() ? menuId : undefined}
        class="flex-none focus:outline-none focus-visible:ring-2"
        style={{
          width: '24px', height: '24px', padding: 0, border: 'none', 'border-radius': '50%',
          background: 'var(--color-bg-panel)', color: props.offReason ? 'var(--color-text-tertiary)' : 'var(--color-text-secondary)',
          cursor: props.offReason ? 'not-allowed' : 'pointer', display: 'grid', 'place-items': 'center',
        }}
        onClick={(e) => {
          e.stopPropagation();
          if (props.offReason) return;
          if (open()) close(false);
          else openMenu();
        }}
        onKeyDown={(e) => {
          if (props.offReason) return;
          if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault();
            e.stopPropagation();
            openMenu();
          }
        }}
      >
        <IconChevronDown size={13} />
      </button>
      <Show when={props.offReason}>
        <span class="sr-only">{props.offReason}</span>
      </Show>
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
              'z-index': 70, 'min-width': '12rem', 'max-width': '18rem', padding: '4px',
              background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)',
              'border-radius': '14px', 'box-shadow': 'var(--shadow-lg)',
            }}
          >
            <For each={props.choices}>
              {(choice) => (
                <button
                  type="button"
                  role="menuitem"
                  tabindex={-1}
                  data-testid={`move-to-${choice.key}`}
                  aria-disabled={choice.reason ? 'true' : undefined}
                  onClick={() => {
                    if (choice.reason) return;
                    close(true);
                    props.onPick(choice.key);
                  }}
                  style={{
                    display: 'block', width: '100%', 'min-height': `${ITEM_HEIGHT}px`, padding: '4px 12px',
                    border: 'none', 'border-radius': '10px', background: 'transparent',
                    cursor: choice.reason ? 'not-allowed' : 'pointer', 'text-align': 'left',
                    'font-size': '0.8125rem', 'font-weight': 500, 'overflow-wrap': 'anywhere',
                    color: choice.reason ? 'var(--color-text-tertiary)' : 'var(--color-text-primary)',
                  }}
                >
                  {choice.label}
                  <Show when={choice.reason}>
                    <span class="block text-caption" style={{ color: 'var(--color-text-tertiary)' }}>{choice.reason}</span>
                  </Show>
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
