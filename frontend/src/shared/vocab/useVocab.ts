import { useProject } from '../state/projectContext';
import {
  resolveLabel,
  getItemTypeList,
  getItemTypeMap,
  type ItemTypeConfig,
} from './vocab';

export interface Vocab {
  /** Translate a vocabulary key to its label (custom or default fallback). */
  t: (key: string) => string;
  /** Item-type configs (value/label/emoji/color) with current labels. */
  types: () => ItemTypeConfig[];
  /** Item-type map keyed by type, with current labels. */
  typeMap: () => Record<string, { label: string; color: string; emoji: string }>;
}

/**
 * Reactive vocabulary bound to the active project. `t()` re-evaluates
 * whenever the project's vocabulary changes, so labels update app-wide the
 * moment Settings saves — no page reload.
 */
export function useVocab(): Vocab {
  const { vocabulary } = useProject();
  return {
    t: (key: string) => resolveLabel(vocabulary(), key),
    types: () => getItemTypeList(vocabulary()),
    typeMap: () => getItemTypeMap(vocabulary()),
  };
}
