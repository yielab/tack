import { createSignal, Show, For, createEffect, onCleanup, type Component } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { api } from '../api';
import type { Item, Priority } from '../types';
import { IconSearch, IconClose } from './icons';
import TypeBadge from './TypeBadge';
import PriorityDot from './PriorityDot';
import KbdHint from './KbdHint';

export interface SearchBarProps {
  projectId?: string;
  placeholder?: string;
}

/** Global / in-project item search. Debounced API query with a token-styled
 *  results dropdown; ↑↓ to navigate, Enter opens the item drawer, ⌃/ focuses. */
const SearchBar: Component<SearchBarProps> = (props) => {
  const navigate = useNavigate();
  const [query, setQuery] = createSignal('');
  const [results, setResults] = createSignal<Item[]>([]);
  const [isOpen, setIsOpen] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [focused, setFocused] = createSignal(false);
  let searchRef: HTMLDivElement | undefined;
  let inputRef: HTMLInputElement | undefined;
  let searchTimeout: number | undefined;

  // ⌃/ (or ⌘/) focuses the search input.
  createEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === '/') {
        e.preventDefault();
        inputRef?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    onCleanup(() => window.removeEventListener('keydown', onKey));
  });

  // Debounced search.
  createEffect(() => {
    const q = query();
    if (q.trim().length < 2) {
      setResults([]);
      setIsOpen(false);
      return;
    }
    if (searchTimeout) clearTimeout(searchTimeout);
    searchTimeout = window.setTimeout(async () => {
      setLoading(true);
      try {
        const items = props.projectId
          ? await api.search.inProject(props.projectId, q)
          : await api.search.global(q);
        setResults(items);
        setIsOpen(items.length > 0);
        setSelectedIndex(0);
      } catch (error) {
        console.error('Search failed:', error);
        setResults([]);
      } finally {
        setLoading(false);
      }
    }, 300);
  });

  // Click outside closes the dropdown.
  createEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (searchRef && !searchRef.contains(e.target as Node)) setIsOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    onCleanup(() => document.removeEventListener('mousedown', onDown));
  });

  const handleKeyDown = (e: KeyboardEvent) => {
    if (!isOpen()) return;
    const items = results();
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, items.length - 1));
        break;
      case 'ArrowUp':
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
        break;
      case 'Enter':
        e.preventDefault();
        if (items[selectedIndex()]) handleSelectItem(items[selectedIndex()]);
        break;
      case 'Escape':
        e.preventDefault();
        setIsOpen(false);
        inputRef?.blur();
        break;
    }
  };

  const handleSelectItem = (item: Item) => {
    // Deep-link to the item drawer over its project board.
    navigate(`/projects/${item.project_id}/board?item=${item.id}`);
    setQuery('');
    setResults([]);
    setIsOpen(false);
    inputRef?.blur();
  };

  const clear = () => {
    setQuery('');
    setResults([]);
    setIsOpen(false);
  };

  return (
    <div ref={searchRef} style={{ position: 'relative', width: '240px' }}>
      {/* search pill */}
      <div
        style={{
          display: 'flex', 'align-items': 'center', gap: '8px',
          padding: '7px 11px', 'border-radius': '9px',
          background: 'var(--color-bg-app)',
          border: '1px solid ' + (focused() ? 'var(--color-accent-line)' : 'var(--color-border-light)'),
          color: 'var(--color-text-secondary)',
        }}
      >
        <Show
          when={loading()}
          fallback={<span style={{ display: 'flex', 'flex-shrink': 0 }}><IconSearch size={14} /></span>}
        >
          <span
            style={{
              width: '14px', height: '14px', 'flex-shrink': 0, 'border-radius': '99px',
              border: '2px solid var(--color-border-light)', 'border-top-color': 'var(--color-primary-600)',
              animation: 'tk-spin .6s linear infinite',
            }}
          />
        </Show>
        <input
          ref={inputRef}
          type="text"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          onFocus={() => { setFocused(true); if (results().length > 0) setIsOpen(true); }}
          onBlur={() => setFocused(false)}
          placeholder={props.placeholder ?? 'Search items…'}
          style={{
            flex: 1, 'min-width': 0, border: 'none', outline: 'none', background: 'transparent',
            'font-family': 'inherit', 'font-size': '12.5px', color: 'var(--color-text-primary)',
          }}
        />
        <Show when={query()} fallback={<KbdHint>⌃/</KbdHint>}>
          <button
            onClick={clear}
            aria-label="Clear search"
            style={{ display: 'flex', background: 'transparent', border: 'none', cursor: 'pointer', color: 'var(--color-text-tertiary)', padding: 0 }}
          >
            <IconClose size={13} />
          </button>
        </Show>
      </div>

      {/* results dropdown */}
      <Show when={isOpen() && results().length > 0}>
        <div
          style={{
            position: 'absolute', 'z-index': 50, width: '360px', 'max-width': '78vw', right: 0, 'margin-top': '8px',
            background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)',
            'border-radius': '13px', 'box-shadow': 'var(--shadow-lg)', overflow: 'hidden',
            animation: 'tk-pal .16s cubic-bezier(.2,.7,.3,1)',
          }}
        >
          <div style={{ 'max-height': '50vh', 'overflow-y': 'auto', padding: '6px' }}>
            <For each={results()}>
              {(item, index) => {
                const active = () => index() === selectedIndex();
                return (
                  <button
                    onClick={() => handleSelectItem(item)}
                    onMouseEnter={() => setSelectedIndex(index())}
                    style={{
                      width: '100%', display: 'flex', 'flex-direction': 'column', gap: '5px',
                      padding: '9px 10px', 'border-radius': '9px', border: 'none', cursor: 'pointer',
                      'text-align': 'left', 'font-family': 'inherit',
                      background: active() ? 'var(--color-accent-soft)' : 'transparent',
                    }}
                  >
                    <span style={{ 'font-size': '13px', 'font-weight': 600, color: 'var(--color-text-primary)', 'white-space': 'nowrap', overflow: 'hidden', 'text-overflow': 'ellipsis', 'max-width': '100%' }}>
                      {item.title}
                    </span>
                    <span style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
                      <TypeBadge type={item.item_type} />
                      <Show when={item.priority !== 'none'}>
                        <PriorityDot priority={item.priority as Priority} showLabel />
                      </Show>
                      <span style={{ 'font-size': '11px', color: 'var(--color-text-tertiary)' }}>{item.status}</span>
                    </span>
                  </button>
                );
              }}
            </For>
          </div>
          <div style={{ display: 'flex', 'align-items': 'center', gap: '12px', padding: '8px 12px', 'border-top': '1px solid var(--color-border-light)', background: 'var(--color-bg-base)', 'font-size': '11px', color: 'var(--color-text-tertiary)' }}>
            <span style={{ display: 'flex', 'align-items': 'center', gap: '5px' }}><KbdHint>↑↓</KbdHint>navigate</span>
            <span style={{ display: 'flex', 'align-items': 'center', gap: '5px' }}><KbdHint>↵</KbdHint>open</span>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default SearchBar;
