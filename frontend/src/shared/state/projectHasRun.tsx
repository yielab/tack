import { createContext, createEffect, createResource, createSignal, onCleanup, useContext, type Accessor, type ParentComponent } from 'solid-js';
import { useParams } from '@solidjs/router';
import { executionsApi } from '../execution';
import { useProjectItems } from './projectItemsContext';

// Whether any agent has ever worked in the project, for the views that have nothing to say before
// one has (Orchestration). Read once per project and kept: a run does not un-happen, so a project
// that has one never flickers back to "none" while a refetch is in flight.

const HasRunContext = createContext<Accessor<boolean | undefined>>();

/** Items asked about in one call, so the address stays short however large the project is. */
const CHUNK = 100;

/** How often a project with no run yet is asked again, so the tab turns on after its first run. */
const RECHECK_MS = 15_000;

async function anyRun(projectId: string, itemIds: readonly string[]): Promise<boolean> {
  // The call that lists the open runs and the day's finished ones answers most projects at once.
  const recent = await executionsApi.listByProject(projectId);
  if ((recent.data.data ?? []).length > 0) return true;
  for (let i = 0; i < itemIds.length; i += CHUNK) {
    const page = await executionsApi.list(undefined, undefined, itemIds.slice(i, i + CHUNK));
    if ((page.data.data ?? []).length > 0) return true;
  }
  return false;
}

export const ProjectHasRunProvider: ParentComponent = (props) => {
  const params = useParams();
  const { items } = useProjectItems();
  const [seen, setSeen] = createSignal<{ project: string; value: boolean } | null>(null);
  const [tick, setTick] = createSignal(0);

  // The items' ids decide what is asked, but a refetch that adds nothing must not ask again.
  const source = () => {
    const list = items();
    return list && params.id ? { project: params.id, count: list.length, tick: tick() } : null;
  };
  const [checked] = createResource(source, async (s) => {
    const ids = (items() ?? []).map((item) => item.id);
    try {
      return { project: s.project, value: await anyRun(s.project, ids) };
    } catch {
      return { project: s.project, value: false };
    }
  });

  createEffect(() => {
    const result = checked();
    if (!result) return;
    const prior = seen();
    // A "yes" is kept; a "no" for another project than the one shown is dropped.
    if (result.value || !prior || prior.project !== result.project) setSeen(result);
  });

  const hasRun = (): boolean | undefined => {
    const s = seen();
    return s && s.project === params.id ? s.value : undefined;
  };

  createEffect(() => {
    if (hasRun() !== false) return;
    const timer = setInterval(() => setTick((n) => n + 1), RECHECK_MS);
    onCleanup(() => clearInterval(timer));
  });

  return <HasRunContext.Provider value={hasRun}>{props.children}</HasRunContext.Provider>;
};

/** `true` once the project has had a run, `false` when it has not, `undefined` while that is not known. */
export function useProjectHasRun(): Accessor<boolean | undefined> {
  return useContext(HasRunContext) ?? (() => undefined);
}
