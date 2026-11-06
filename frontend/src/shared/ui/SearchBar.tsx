import { createSignal, Show, For, createEffect, onCleanup, type Component } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import type { Item } from '../types';
import { IconSearch, IconClose } from './icons';
import { createItemSearch, ItemResult } from './itemSearch';
import KbdHint from './KbdHint';

export interface SearchBarProps {
  projectId?: string;
  placeholder?: string;
}

/** Global / in-project item search. Debounced API query with a token-styled
 *  results dropdown; each result shows its type, the path of what it sits under, priority
 *  and status. ↑↓ to navigate, Enter opens the item drawer, ⌃/ focuses. */
const SearchBar: Component<SearchBarProps> = (props) => {
  const navigate = useNavigate();
  const { query, setQuery, results, loading, byId, clear: clearSearch } = createItemSearch(() => props.projectId);
  const [isOpen, setIsOpen] = createSignal(false);
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [focused, setFocused] = createSignal(false);
  let searchRef: HTMLDivElement | undefined;
  let inputRef: HTMLInputElement | undefined;

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

  // The dropdown opens when a search finds something, and closes when the query is too short to search.
  createEffect(() => {
    if (query().trim().length < 2) setIsOpen(false);
  });
  createEffect(() => {
    const found = results();
    setSelectedIndex(0);
    if (found.length > 0) setIsOpen(true);
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
    clear();
    inputRef?.blur();
  };

  const clear = () => {
    clearSearch();
    setIsOpen(false);
  };

  return (
    <div ref={searchRef} style={{ position: 'relative', width: '240px' }}>
      {/* search pill */}
      <div
        style={{
          display: 'flex', 'align-items': 'center', gap: '8px',
          padding: '7px 14px', 'border-radius': 'var(--radius-pill)',
          background: 'var(--color-bg-panel)',
          border: '1px solid ' + (focused() ? 'var(--color-primary-600)' : 'transparent'),
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
          role="combobox"
          aria-expanded={isOpen() && results().length > 0 ? 'true' : 'false'}
          aria-controls="search-results"
          aria-autocomplete="list"
          aria-activedescendant={isOpen() && results().length > 0 ? `search-option-${selectedIndex()}` : undefined}
          aria-label={props.placeholder ?? 'Search items'}
          placeholder={props.placeholder ?? 'Search items…'}
          style={{
            flex: 1, 'min-width': 0, border: 'none', outline: 'none', background: 'transparent',
            'font-family': 'inherit', 'font-size': '0.8125rem', color: 'var(--color-text-primary)',
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
            'border-radius': 'var(--radius-xl)', 'box-shadow': 'var(--shadow-lg)', overflow: 'hidden',
            animation: 'tk-pal .16s cubic-bezier(.2,.7,.3,1)',
          }}
        >
          <div
            id="search-results"
            role="listbox"
            aria-label="Search results"
            style={{ 'max-height': '50vh', 'overflow-y': 'auto', padding: '6px' }}
            // A path link inside a result goes to that item: the results have done their job.
            on:click={(e) => { if ((e.target as Element).closest('a')) clear(); }}
          >
            <For each={results()}>
              {(item, index) => {
                const active = () => index() === selectedIndex();
                return (
                  <div
                    id={`search-option-${index()}`}
                    role="option"
                    aria-selected={active() ? 'true' : 'false'}
                    data-testid="search-result"
                    onClick={() => handleSelectItem(item)}
                    onMouseEnter={() => setSelectedIndex(index())}
                    style={{
                      width: '100%', display: 'flex', 'flex-direction': 'column', gap: '5px',
                      padding: '9px 12px', 'border-radius': 'var(--radius-lg)', cursor: 'pointer',
                      'text-align': 'left', 'box-sizing': 'border-box',
                      background: active() ? 'var(--color-accent-soft)' : 'transparent',
                    }}
                  >
                    <ItemResult item={item} byId={byId()} />
                  </div>
                );
              }}
            </For>
          </div>
          <div style={{ display: 'flex', 'align-items': 'center', gap: '12px', padding: '8px 12px', 'border-top': '1px solid var(--color-border-light)', background: 'var(--color-bg-base)', 'font-size': '0.6875rem', color: 'var(--color-text-tertiary)' }}>
            <span style={{ display: 'flex', 'align-items': 'center', gap: '5px' }}><KbdHint>↑↓</KbdHint>navigate</span>
            <span style={{ display: 'flex', 'align-items': 'center', gap: '5px' }}><KbdHint>↵</KbdHint>open</span>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default SearchBar;
