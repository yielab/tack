import { Show } from 'solid-js';
import { A, useLocation } from '@solidjs/router';
import { useProject } from '../state/projectContext';
import { useVocab } from '../vocab/useVocab';

// Work lenses (rendered under "Work"). `sprint` matches the singular `/sprint`
// route and is relabelled via the project vocabulary.
const LENS_LABELS: Record<string, string> = {
  board: 'Board', list: 'List', table: 'Table', calendar: 'Calendar', timeline: 'Timeline', sprint: 'Sprints',
};
const DEST_LABELS: Record<string, string> = {
  overview: 'Overview', settings: 'Settings',
};

export default function Breadcrumb() {
  const location = useLocation();
  const { project } = useProject();
  const { t } = useVocab();

  const section = (): string | null => {
    const p = location.pathname;
    const lens = Object.keys(LENS_LABELS).find(l => p.endsWith(`/${l}`) || p.includes(`/${l}/`));
    if (lens) return lens === 'sprint' ? t('sprint') : LENS_LABELS[lens];
    const dest = Object.keys(DEST_LABELS).find(d => p.endsWith(`/${d}`));
    if (dest) return DEST_LABELS[dest];
    return null;
  };

  const isWork = (): boolean => {
    const p = location.pathname;
    return Object.keys(LENS_LABELS).some(l => p.endsWith(`/${l}`) || p.includes(`/${l}/`));
  };

  return (
    <nav class="flex items-center gap-1 text-sm min-w-0">
      <Show when={project()}>
        <A
          href="/projects"
          class="shrink-0 font-medium transition-colors hover:underline"
          style={{ color: 'var(--color-text-tertiary)' }}
        >
          Projects
        </A>
        <span style={{ color: 'var(--color-text-tertiary)' }} class="shrink-0 mx-0.5">›</span>
        <span
          class="font-semibold truncate max-w-[140px]"
          style={{ color: 'var(--color-text-primary)' }}
          title={project()?.name}
        >
          {project()?.name}
        </span>
        <Show when={section()}>
          <span style={{ color: 'var(--color-text-tertiary)' }} class="shrink-0 mx-0.5">›</span>
          <Show when={isWork()}>
            <span style={{ color: 'var(--color-text-tertiary)' }} class="shrink-0">Work</span>
            <span style={{ color: 'var(--color-text-tertiary)' }} class="shrink-0 mx-0.5">›</span>
          </Show>
          <span style={{ color: 'var(--color-primary-600)' }} class="font-semibold shrink-0">
            {section()}
          </span>
        </Show>
      </Show>
      <Show when={!project()}>
        <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          {location.pathname === '/templates' ? 'Templates' : 'Projects'}
        </span>
      </Show>
    </nav>
  );
}
