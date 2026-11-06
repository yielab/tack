import { createSignal, createEffect, onCleanup, Show, type Accessor, type Component } from 'solid-js';
import { api } from '../api';
import type { Item, Priority } from '../types';
import TypeTile from './TypeTile';
import ItemPath from './ItemPath';
import PriorityDot from './PriorityDot';

/**
 * The state behind a box that searches items: the query, its debounced results, and the items those
 * results sit under, read from each result's project so `ItemPath` can name them. Search sends the
 * matches only, so the ancestors are fetched once per project for as long as results are shown.
 */
export function createItemSearch(projectId: Accessor<string | undefined>) {
  const [query, setQuery] = createSignal('');
  const [results, setResults] = createSignal<Item[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [projects, setProjects] = createSignal<ReadonlyMap<string, Item[]>>(new Map());
  let timer: number | undefined;
  let latest = 0;

  const byId = (): ReadonlyMap<string, Item> => {
    const all = new Map<string, Item>();
    for (const items of projects().values()) for (const item of items) all.set(item.id, item);
    for (const item of results()) all.set(item.id, item);
    return all;
  };

  const clear = () => {
    latest += 1;
    window.clearTimeout(timer);
    setQuery('');
    setResults([]);
    setProjects(new Map());
    setLoading(false);
  };

  const loadAncestors = async (found: Item[], ticket: number) => {
    const wanted = [...new Set(found.filter((i) => i.parent_id).map((i) => i.project_id))].filter((id) => !projects().has(id));
    for (const id of wanted) {
      try {
        const items = await api.items.list(id);
        if (ticket !== latest) return;
        setProjects((prior) => new Map(prior).set(id, items));
      } catch {
        // The path is a nicety: results stay usable without it.
      }
    }
  };

  createEffect(() => {
    const q = query();
    const scope = projectId();
    window.clearTimeout(timer);
    if (q.trim().length < 2) {
      latest += 1;
      setResults([]);
      setLoading(false);
      return;
    }
    timer = window.setTimeout(async () => {
      const ticket = ++latest;
      setLoading(true);
      try {
        const found = scope ? await api.search.inProject(scope, q) : await api.search.global(q);
        if (ticket !== latest) return;
        setResults(found);
        void loadAncestors(found, ticket);
      } catch (error) {
        if (ticket !== latest) return;
        console.error('Search failed:', error);
        setResults([]);
      } finally {
        if (ticket === latest) setLoading(false);
      }
    }, 300);
  });
  onCleanup(() => window.clearTimeout(timer));

  return { query, setQuery, results, loading, byId, clear };
}

/**
 * One found item: its title, then its type, the path of what it sits under, its priority and its
 * status. It is the body of a result row; the row decides what a click does.
 */
export const ItemResult: Component<{ item: Item; byId: ReadonlyMap<string, Item>; active?: boolean }> = (props) => (
  <>
    <span
      style={{
        'font-size': '0.8125rem',
        'font-weight': 600,
        color: 'var(--color-text-primary)',
        'white-space': 'nowrap',
        overflow: 'hidden',
        'text-overflow': 'ellipsis',
        'max-width': '100%',
      }}
    >
      {props.item.title}
    </span>
    <span style={{ display: 'flex', 'align-items': 'center', gap: '8px', 'flex-wrap': 'wrap', 'min-width': 0 }}>
      <TypeTile type={props.item.item_type} />
      <Show when={props.item.priority !== 'none'}>
        <PriorityDot priority={props.item.priority as Priority} showLabel />
      </Show>
      <span style={{ 'font-size': '0.6875rem', color: 'var(--color-text-tertiary)' }}>{props.item.status}</span>
    </span>
    <ItemPath item={props.item} byId={props.byId} projectId={props.item.project_id} />
  </>
);
