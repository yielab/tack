import { createSignal, For, Show, type Component, createEffect, createMemo, onCleanup } from 'solid-js';
import { Portal, Dynamic } from 'solid-js/web';
import { IconSearch, type IconProps } from './icons';
import KbdHint from './KbdHint';
import { trapFocus } from './focusTrap';

export interface Command {
  id: string;
  label: string;
  description?: string;
  /** The same icon the sidebar uses for the place, when there is one. */
  icon?: Component<IconProps>;
  /** Section heading in the palette (e.g. "Go to", "Actions"). */
  group?: string;
  action: () => void;
  shortcut?: string;
}

export interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  commands: Command[];
}

const CommandPalette: Component<CommandPaletteProps> = (props) => {
  const [search, setSearch] = createSignal('');
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  let inputRef: HTMLInputElement | undefined;
  let dialog: HTMLDivElement | undefined;

  createEffect(() => {
    if (!props.isOpen) return;
    setSearch('');
    setSelectedIndex(0);
    const previouslyFocused = document.activeElement as HTMLElement | null;
    setTimeout(() => inputRef?.focus(), 10);
    // Escape closes from anywhere in the palette, not only from the input.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') props.onClose();
      trapFocus(dialog, e);
    };
    document.addEventListener('keydown', onKey);
    onCleanup(() => {
      document.removeEventListener('keydown', onKey);
      previouslyFocused?.focus?.();
    });
  });

  // Flat, filtered list (drives ↑↓ selection).
  const filtered = createMemo(() => {
    const q = search().toLowerCase();
    if (!q) return props.commands;
    return props.commands.filter(
      (c) => c.label.toLowerCase().includes(q) || c.description?.toLowerCase().includes(q),
    );
  });

  // Same list, grouped for display, preserving the flat order/index.
  const groups = createMemo(() => {
    const out: { title: string; items: { cmd: Command; index: number }[] }[] = [];
    filtered().forEach((cmd, index) => {
      const title = cmd.group ?? 'Commands';
      let g = out.find((x) => x.title === title);
      if (!g) { g = { title, items: [] }; out.push(g); }
      g.items.push({ cmd, index });
    });
    return out;
  });

  createEffect(() => {
    const max = Math.max(0, filtered().length - 1);
    if (selectedIndex() > max) setSelectedIndex(max);
  });

  const handleKeyDown = (e: KeyboardEvent) => {
    const cmds = filtered();
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, cmds.length - 1));
        break;
      case 'ArrowUp':
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
        break;
      case 'Enter':
        e.preventDefault();
        if (cmds[selectedIndex()]) { cmds[selectedIndex()].action(); props.onClose(); }
        break;
    }
  };

  const run = (cmd: Command) => { cmd.action(); props.onClose(); };
  const handleBackdrop = (e: MouseEvent) => { if (e.target === e.currentTarget) props.onClose(); };

  return (
    <Show when={props.isOpen}>
      <Portal>
        <div
          onClick={handleBackdrop}
          style={{
            position: 'fixed', inset: 0, 'z-index': 70, display: 'flex',
            'align-items': 'flex-start', 'justify-content': 'center', 'padding-top': '12vh',
            'background-color': 'var(--color-bg-overlay)', animation: 'tk-overlay .12s ease',
          }}
        >
          <div
            ref={dialog}
            role="dialog"
            aria-modal="true"
            aria-label="Command palette"
            tabindex={-1}
            onClick={(e) => e.stopPropagation()}
            style={{
              outline: 'none',
              width: '560px', 'max-width': '92vw', background: 'var(--color-bg-elevated)',
              'border-radius': 'var(--radius-card)',
              'box-shadow': 'var(--shadow-lg)', overflow: 'hidden',
              animation: 'tk-pal .16s cubic-bezier(.2,.7,.3,1)',
            }}
          >
            {/* search header */}
            <div style={{ padding: '14px 14px 6px' }}>
            <div style={{ display: 'flex', 'align-items': 'center', gap: '10px', padding: '9px 10px 9px 16px', 'border-radius': 'var(--radius-pill)', background: 'var(--color-bg-app)' }}>
              <span aria-hidden="true" style={{ color: 'var(--color-text-tertiary)', display: 'flex' }}><IconSearch size={17} /></span>
              <input
                ref={inputRef}
                role="combobox"
                aria-label="Search items, views and commands"
                aria-expanded="true"
                aria-controls="cmdk-results"
                aria-autocomplete="list"
                aria-activedescendant={filtered().length > 0 ? `cmdk-option-${selectedIndex()}` : undefined}
                value={search()}
                onInput={(e) => setSearch(e.currentTarget.value)}
                onKeyDown={handleKeyDown}
                placeholder="Search items, jump to a view, or run a command…"
                style={{
                  flex: 1, border: 'none', outline: 'none', background: 'transparent',
                  'font-family': 'inherit', 'font-size': '0.90625rem', color: 'var(--color-text-primary)',
                }}
              />
              <KbdHint>esc</KbdHint>
            </div>
            </div>

            {/* results */}
            <div id="cmdk-results" role="listbox" aria-label="Results" style={{ 'max-height': '50vh', 'overflow-y': 'auto', padding: '6px 10px 10px' }}>
              <Show
                when={filtered().length > 0}
                fallback={<div role="status" style={{ padding: '28px', 'text-align': 'center', 'font-size': '0.8125rem', color: 'var(--color-text-tertiary)' }}>No matches for “{search()}”</div>}
              >
                <For each={groups()}>
                  {(g, gi) => (
                    <div role="group" aria-labelledby={`cmdk-group-${gi()}`}>
                      <div style={{ padding: '8px 8px 4px' }}>
                        <span id={`cmdk-group-${gi()}`} style={{ 'font-size': '0.65625rem', 'font-weight': 700, 'letter-spacing': '.1em', 'text-transform': 'uppercase', color: 'var(--color-text-tertiary)' }}>{g.title}</span>
                      </div>
                      <For each={g.items}>
                        {({ cmd, index }) => {
                          const active = () => index === selectedIndex();
                          return (
                            <button
                              type="button"
                              id={`cmdk-option-${index}`}
                              role="option"
                              aria-selected={active() ? 'true' : 'false'}
                              tabindex={-1}
                              onClick={() => run(cmd)}
                              onMouseEnter={() => setSelectedIndex(index)}
                              style={{
                                width: '100%', display: 'flex', 'align-items': 'center', gap: '10px',
                                padding: '7px 12px 7px 7px', 'border-radius': 'var(--radius-pill)', border: 'none', cursor: 'pointer',
                                'text-align': 'left', 'font-family': 'inherit',
                                background: active() ? 'var(--color-accent-soft)' : 'transparent',
                              }}
                            >
                              <span aria-hidden="true" style={{
                                width: '26px', height: '26px', 'border-radius': 'var(--radius-pill)', 'flex-shrink': 0,
                                display: 'flex', 'align-items': 'center', 'justify-content': 'center', 'font-size': '0.75rem',
                                background: active() ? 'var(--color-bg-base)' : 'var(--color-chip)',
                              }}>{cmd.icon ? <Dynamic component={cmd.icon} size={15} /> : '•'}</span>
                              <span style={{ flex: 1, 'font-size': '0.84375rem', 'font-weight': active() ? 600 : 500, color: active() ? 'var(--color-accent-ink)' : 'var(--color-text-primary)' }}>{cmd.label}</span>
                              <Show when={cmd.shortcut}>
                                <span style={{ 'font-family': 'var(--font-mono)', 'font-size': '0.65625rem', color: 'var(--color-text-tertiary)' }}>{cmd.shortcut}</span>
                              </Show>
                            </button>
                          );
                        }}
                      </For>
                    </div>
                  )}
                </For>
              </Show>
            </div>

            {/* footer legend */}
            <div style={{ display: 'flex', 'align-items': 'center', gap: '14px', padding: '10px 20px', 'border-top': '1px solid var(--color-border-light)', background: 'var(--color-bg-panel)', 'font-size': '0.6875rem', color: 'var(--color-text-tertiary)' }}>
              <span style={{ display: 'flex', 'align-items': 'center', gap: '5px' }}>
                <KbdHint>↑↓</KbdHint>navigate
              </span>
              <span style={{ display: 'flex', 'align-items': 'center', gap: '5px' }}>
                <KbdHint>↵</KbdHint>select
              </span>
              <div style={{ flex: 1 }} />
              <span>Tack · ⌃K anywhere</span>
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  );
};

export default CommandPalette;
