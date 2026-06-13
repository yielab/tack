import { createSignal, Show, For, createEffect, onCleanup, type Component } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { api } from '../api';
import type { Item } from '../types';

export interface SearchBarProps {
  projectId?: string;
  placeholder?: string;
}

const SearchBar: Component<SearchBarProps> = (props) => {
  const navigate = useNavigate();
  const [query, setQuery] = createSignal('');
  const [results, setResults] = createSignal<Item[]>([]);
  const [isOpen, setIsOpen] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  let searchRef: HTMLDivElement | undefined;
  let inputRef: HTMLInputElement | undefined;
  let searchTimeout: number | undefined;

  // Global keyboard shortcut (Ctrl+/)
  createEffect(() => {
    const handleGlobalKeydown = (e: KeyboardEvent) => {
      // Ctrl+/ or Cmd+/ to focus search
      if ((e.ctrlKey || e.metaKey) && e.key === '/') {
        e.preventDefault();
        inputRef?.focus();
      }
    };

    window.addEventListener('keydown', handleGlobalKeydown);
    onCleanup(() => {
      window.removeEventListener('keydown', handleGlobalKeydown);
    });
  });

  // Debounced search
  createEffect(() => {
    const q = query();
    if (q.trim().length < 2) {
      setResults([]);
      setIsOpen(false);
      return;
    }

    if (searchTimeout) {
      clearTimeout(searchTimeout);
    }

    searchTimeout = window.setTimeout(async () => {
      setLoading(true);
      try {
        let items: Item[];
        if (props.projectId) {
          items = await api.search.inProject(props.projectId, q);
        } else {
          items = await api.search.global(q);
        }
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

  // Click outside to close
  createEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (searchRef && !searchRef.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    onCleanup(() => {
      document.removeEventListener('mousedown', handleClickOutside);
    });
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
        if (items[selectedIndex()]) {
          handleSelectItem(items[selectedIndex()]);
        }
        break;
      case 'Escape':
        e.preventDefault();
        setIsOpen(false);
        inputRef?.blur();
        break;
    }
  };

  const handleSelectItem = (item: Item) => {
    // Navigate to board view with the item's project
    navigate(`/projects/${item.project_id}/board`);
    setQuery('');
    setResults([]);
    setIsOpen(false);
    inputRef?.blur();
  };

  const formatItemType = (type: Item['item_type']): string => {
    if (typeof type === 'string') {
      return type;
    }
    return 'custom';
  };

  const getPriorityColor = (priority: string) => {
    switch (priority) {
      case 'critical':
      case 'high':
        return 'text-danger-600';
      case 'medium':
        return 'text-warning-600';
      case 'low':
      case 'none':
        return 'text-content-subtle';
      default:
        return 'text-content-subtle';
    }
  };

  return (
    <div ref={searchRef} class="relative w-full max-w-md">
      <div class="relative">
        <input
          ref={inputRef}
          type="text"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          onFocus={() => {
            if (results().length > 0) {
              setIsOpen(true);
            }
          }}
          placeholder={props.placeholder || 'Search items...'}
          class="w-full pl-10 pr-4 py-2 border border-line-medium rounded-lg bg-elevated text-content placeholder-content-faint focus:ring-2 focus:ring-brand-500 focus:border-transparent"
        />
        <div class="absolute left-3 top-1/2 -translate-y-1/2 text-content-subtle">
          {loading() ? (
            <svg class="animate-spin h-5 w-5" fill="none" viewBox="0 0 24 24">
              <circle
                class="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                stroke-width="4"
              />
              <path
                class="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
              />
            </svg>
          ) : (
            <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
              />
            </svg>
          )}
        </div>
        <Show when={query()}>
          <button
            onClick={() => {
              setQuery('');
              setResults([]);
              setIsOpen(false);
            }}
            class="absolute right-3 top-1/2 -translate-y-1/2 text-content-subtle hover:text-content-muted"
          >
            <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </Show>
      </div>

      {/* Results Dropdown */}
      <Show when={isOpen() && results().length > 0}>
        <div class="absolute z-50 w-full mt-2 bg-elevated rounded-lg shadow-lg border border-line max-h-96 overflow-y-auto">
          <For each={results()}>
            {(item, index) => (
              <button
                onClick={() => handleSelectItem(item)}
                onMouseEnter={() => setSelectedIndex(index())}
                class="w-full px-4 py-3 text-left hover:bg-sunken transition-colors border-b border-line last:border-0"
                classList={{
                  'bg-brand-50': index() === selectedIndex(),
                }}
              >
                <div class="flex items-start justify-between gap-3">
                  <div class="flex-1 min-w-0">
                    <h4 class="font-medium text-content truncate">
                      {item.title}
                    </h4>
                    <Show when={item.description}>
                      <p class="text-sm text-content-muted line-clamp-1 mt-1">
                        {item.description}
                      </p>
                    </Show>
                    <div class="flex items-center gap-2 mt-2">
                      <span class="text-xs px-2 py-0.5 rounded bg-sunken text-content-muted">
                        {formatItemType(item.item_type)}
                      </span>
                      <span class={`text-xs font-medium ${getPriorityColor(item.priority)}`}>
                        {item.priority}
                      </span>
                      <span class="text-xs text-content-subtle">
                        {item.status}
                      </span>
                    </div>
                  </div>
                </div>
              </button>
            )}
          </For>
          <div class="px-4 py-2 text-xs text-content-subtle bg-sunken border-t border-line">
            <span>Use ↑ ↓ to navigate · Enter to select · Esc to close</span>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default SearchBar;
