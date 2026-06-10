import { For } from 'solid-js';
import { A, useParams, useLocation } from '@solidjs/router';
import { setLastLens, type Lens } from '../state/lastView';

const TABS: { lens: Lens; label: string; icon: string }[] = [
  { lens: 'board',    label: 'Board',    icon: '⬛' },
  { lens: 'list',     label: 'List',     icon: '☰'  },
  { lens: 'tree',     label: 'Tree',     icon: '🌲' },
  { lens: 'calendar', label: 'Calendar', icon: '📅' },
  { lens: 'timeline', label: 'Timeline', icon: '📊' },
];

export default function WorkTabs() {
  const params = useParams();
  const location = useLocation();
  const projectId = () => params.id;

  const activeLens = (): Lens => {
    const p = location.pathname;
    if (p.includes('/list'))     return 'list';
    if (p.includes('/tree'))     return 'tree';
    if (p.includes('/calendar')) return 'calendar';
    if (p.includes('/timeline')) return 'timeline';
    return 'board';
  };

  return (
    <div
      class="flex items-center gap-1 px-1 py-1 rounded-lg"
      style={{ background: 'var(--color-bg-subtle)', border: '1px solid var(--color-border-light)' }}
    >
      <For each={TABS}>
        {(tab) => (
          <A
            href={`/projects/${projectId()}/${tab.lens}`}
            onClick={() => setLastLens(tab.lens)}
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium transition-all"
            style={activeLens() === tab.lens
              ? {
                  background: 'var(--color-bg-base)',
                  color: 'var(--color-primary-600)',
                  'box-shadow': '0 1px 3px var(--color-shadow)',
                }
              : {
                  color: 'var(--color-text-secondary)',
                }}
          >
            <span class="text-xs">{tab.icon}</span>
            {tab.label}
          </A>
        )}
      </For>
    </div>
  );
}
