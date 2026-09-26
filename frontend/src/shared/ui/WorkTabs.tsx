import { For } from 'solid-js';
import { A, useParams, useLocation } from '@solidjs/router';
import { setLastLens, type Lens } from '../state/lastView';
import { useVocab } from '../vocab/useVocab';

// `vocab` marks lenses whose label is resolved from the project vocabulary
// (e.g. `sprint` → "Phase" for a construction project).
const TABS: { lens: Lens; label: string; vocab?: string }[] = [
  { lens: 'board',    label: 'Board' },
  { lens: 'list',     label: 'List' },
  { lens: 'table',    label: 'Table' },
  { lens: 'calendar', label: 'Calendar' },
  { lens: 'timeline', label: 'Timeline' },
  { lens: 'sprint',   label: 'Sprint', vocab: 'sprint' },
];

export default function WorkTabs() {
  const params = useParams();
  const location = useLocation();
  const { t } = useVocab();
  const projectId = () => params.id;

  const activeLens = (): Lens => {
    const p = location.pathname;
    if (p.includes('/list'))     return 'list';
    if (p.includes('/table'))    return 'table';
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
          <A
            href={`/projects/${projectId()}/${tab.lens}`}
            onClick={() => setLastLens(tab.lens)}
            class="flex items-center px-3.5 py-[5px] rounded-full text-[13px] transition-all"
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
        )}
      </For>
    </div>
  );
}
