import { useContext } from 'solid-js';
import { ProjectContext } from '../state/projectContext';
import type { ItemTypeDef, TypeColor } from '../types';

/** The key with its first letter capitalised: what a type with no catalog entry is called. */
export function capitalised(key: string): string {
  return key.charAt(0).toUpperCase() + key.slice(1);
}

export interface ItemTypes {
  /** The project's types in display order. */
  list: () => ItemTypeDef[];
  /** The type a key names, when the catalog has it. */
  get: (key: string) => ItemTypeDef | undefined;
  /** What the project calls the type, else the capitalised key. */
  name: (key: string) => string;
  color: (key: string) => TypeColor;
  /** The key an item made without a type takes. */
  defaultKey: () => string;
}

/**
 * The active project's type catalog (`Project.item_types`), reactive like the project itself.
 * Outside a project it is empty and every key reads as its capitalised self, so a component that
 * shows a type works anywhere.
 */
export function useItemTypes(): ItemTypes {
  const ctx = useContext(ProjectContext);
  const list = () => ctx?.project()?.item_types ?? [];
  const get = (key: string) => list().find((t) => t.key === key);
  return {
    list,
    get,
    name: (key) => get(key)?.name ?? capitalised(key),
    color: (key) => get(key)?.color ?? 'neutral',
    defaultKey: () => list().find((t) => t.default_work)?.key ?? 'task',
  };
}
