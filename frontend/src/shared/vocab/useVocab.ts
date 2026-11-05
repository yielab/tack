import { useProject } from '../state/projectContext';
import { resolveLabel } from './vocab';

export interface Vocab {
  /** Translate a vocabulary key to its label (custom or default fallback). */
  t: (key: string) => string;
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
  };
}
