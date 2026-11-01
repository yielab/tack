import { For, Show } from 'solid-js';
import { A, useParams, useLocation } from '@solidjs/router';
import { setLastLens, type Lens } from '../state/lastView';
import { useVocab } from '../vocab/useVocab';
import { useProjectHasRun } from '../state/projectHasRun';

// `vocab` marks lenses whose label is resolved from the project vocabulary
// (e.g. `sprint` → "Phase" for a construction project). Orchestration is the same, and is the one
// tab that waits for something to show: it is off until an agent has worked in the project.
const TABS: { lens: Lens; label: string; vocab?: string }[] = [
  { lens: 'board',    label: 'Board' },
  { lens: 'orchestration', label: 'Orchestration', vocab: 'orchestration' },
  { lens: 'list',     label: 'List' },
  { lens: 'calendar', label: 'Calendar' },
  { lens: 'timeline', label: 'Timeline' },
  { lens: 'sprint',   label: 'Sprint', vocab: 'sprint' },
];

/** Said on the Orchestration tab while it is off, and by the page itself when it is opened anyway. */
export const NO_AGENT_YET = 'No agent has worked in this project yet';

export default function WorkTabs() {
  const params = useParams();
  const location = useLocation();
  const { t } = useVocab();
  const projectId = () => params.id;
  const hasRun = useProjectHasRun();
  // Off only when it is known that nothing has run; while that is unknown the tab stays a link.
  const offReason = (lens: Lens) => (lens === 'orchestration' && hasRun() === false ? NO_AGENT_YET : null);

  const activeLens = (): Lens => {
    const p = location.pathname;
    if (p.includes('/orchestration')) return 'orchestration';
    if (p.includes('/list'))     return 'list';
    if (p.includes('/calendar')) return 'calendar';
    if (p.includes('/timeline')) return 'timeline';
    if (p.includes('/sprint'))   return 'sprint';
    return 'board';
  };

  return (
    <div
      class="inline-flex items-center gap-[3px] p-[3px] rounded-full"
      style={{ background: 'var(--color-bg-panel)' }}
    >
      <For each={TABS}>
        {(tab) => (
          <Show
            when={!offReason(tab.lens)}
            fallback={
              <span
                data-testid={`tab-${tab.lens}-off`}
                aria-disabled="true"
                title={offReason(tab.lens) ?? undefined}
                class="flex items-center px-3.5 rounded-full text-small"
                style={{ 'padding-block': '5px', color: 'var(--color-text-tertiary)', 'font-weight': 500, cursor: 'not-allowed' }}
              >
                {tab.vocab ? t(tab.vocab) : tab.label}
                <span class="sr-only">{`. ${offReason(tab.lens)}`}</span>
              </span>
            }
          >
          <A
            href={`/projects/${projectId()}/${tab.lens}`}
            onClick={() => setLastLens(tab.lens)}
            class="flex items-center px-3.5 py-[5px] rounded-full text-small transition-all"
            style={activeLens() === tab.lens
              ? {
                  background: 'var(--color-bg-app)',
                  color: 'var(--color-accent-ink)',
                  'font-weight': 700,
                  'box-shadow': 'var(--shadow-sm)',
                }
              : {
                  color: 'var(--color-text-primary)',
                  'font-weight': 500,
                }}
          >
            {tab.vocab ? t(tab.vocab) : tab.label}
          </A>
          </Show>
        )}
      </For>
    </div>
  );
}
