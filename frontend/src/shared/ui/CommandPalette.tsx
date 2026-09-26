import { createSignal, For, Show, type Component, createEffect, createMemo } from 'solid-js';
import { Portal } from 'solid-js/web';
import { IconSearch } from './icons';
import KbdHint from './KbdHint';

export interface Command {
  id: string;
  label: string;
  description?: string;
  icon?: string;
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

  createEffect(() => {
    if (props.isOpen) {
      setSearch('');
      setSelectedIndex(0);
      setTimeout(() => inputRef?.focus(), 10);
    }
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
      case 'Escape':
        e.preventDefault();
        props.onClose();
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
            onClick={(e) => e.stopPropagation()}
            style={{
              width: '560px', 'max-width': '92vw', background: 'var(--color-bg-elevated)',
              'border-radius': 'var(--radius-card)',
              'box-shadow': 'var(--shadow-lg)', overflow: 'hidden',
              animation: 'tk-pal .16s cubic-bezier(.2,.7,.3,1)',
            }}
          >
            {/* search header */}
            <div style={{ padding: '14px 14px 6px' }}>
            <div style={{ display: 'flex', 'align-items': 'center', gap: '10px', padding: '9px 10px 9px 16px', 'border-radius': 'var(--radius-pill)', background: 'var(--color-bg-app)' }}>
              <span style={{ color: 'var(--color-text-tertiary)', display: 'flex' }}><IconSearch size={17} /></span>
              <input
                ref={inputRef}
                value={search()}
                onInput={(e) => setSearch(e.currentTarget.value)}
                onKeyDown={handleKeyDown}
                placeholder="Search items, jump to a view, or run a command…"
                style={{
                  flex: 1, border: 'none', outline: 'none', background: 'transparent',
                  'font-family': 'inherit', 'font-size': '14.5px', color: 'var(--color-text-primary)',
                }}
              />
              <KbdHint>esc</KbdHint>
            </div>
            </div>

            {/* results */}
            <div style={{ 'max-height': '50vh', 'overflow-y': 'auto', padding: '6px 10px 10px' }}>
              <Show
                when={filtered().length > 0}
                fallback={<div style={{ padding: '28px', 'text-align': 'center', 'font-size': '13px', color: 'var(--color-text-tertiary)' }}>No matches for “{search()}”</div>}
              >
                <For each={groups()}>
                  {(g) => (
                    <>
                      <div style={{ padding: '8px 8px 4px' }}>
                        <span style={{ 'font-size': '10.5px', 'font-weight': 700, 'letter-spacing': '.1em', 'text-transform': 'uppercase', color: 'var(--color-text-tertiary)' }}>{g.title}</span>
                      </div>
                      <For each={g.items}>
                        {({ cmd, index }) => {
                          const active = () => index === selectedIndex();
                          return (
                            <button
                              onClick={() => run(cmd)}
                              onMouseEnter={() => setSelectedIndex(index)}
                              style={{
                                width: '100%', display: 'flex', 'align-items': 'center', gap: '10px',
                                padding: '7px 12px 7px 7px', 'border-radius': 'var(--radius-pill)', border: 'none', cursor: 'pointer',
                                'text-align': 'left', 'font-family': 'inherit',
                                background: active() ? 'var(--color-accent-soft)' : 'transparent',
                              }}
                            >
                              <span style={{
                                width: '26px', height: '26px', 'border-radius': 'var(--radius-pill)', 'flex-shrink': 0,
                                display: 'flex', 'align-items': 'center', 'justify-content': 'center', 'font-size': '12px',
                                background: active() ? 'var(--color-bg-base)' : 'var(--color-chip)',
                              }}>{cmd.icon ?? '•'}</span>
                              <span style={{ flex: 1, 'font-size': '13.5px', 'font-weight': active() ? 600 : 500, color: active() ? 'var(--color-accent-ink)' : 'var(--color-text-primary)' }}>{cmd.label}</span>
                              <Show when={cmd.shortcut}>
                                <span style={{ 'font-family': 'var(--font-mono)', 'font-size': '10.5px', color: 'var(--color-text-tertiary)' }}>{cmd.shortcut}</span>
                              </Show>
                            </button>
                          );
                        }}
                      </For>
                    </>
                  )}
                </For>
              </Show>
            </div>

            {/* footer legend */}
            <div style={{ display: 'flex', 'align-items': 'center', gap: '14px', padding: '10px 20px', 'border-top': '1px solid var(--color-border-light)', background: 'var(--color-bg-panel)', 'font-size': '11px', color: 'var(--color-text-tertiary)' }}>
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
