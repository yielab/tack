import { lazy } from 'solid-js';
import { useLocation } from '@solidjs/router';
import type { RouteDefinition } from '@solidjs/router';
import { Button } from '../shared/ui';
import WorkLayout from './WorkLayout';

const Projects      = lazy(() => import('../features/projects/Projects'));
const Board         = lazy(() => import('../features/board/Board'));
const List          = lazy(() => import('../features/list/List'));
const Table         = lazy(() => import('../features/table/Table'));
const Dashboard     = lazy(() => import('../features/dashboard/Dashboard'));
const Sprints       = lazy(() => import('../features/sprints/Sprints'));
const Calendar      = lazy(() => import('../features/calendar/Calendar'));
const Timeline      = lazy(() => import('../features/timeline/Timeline'));
const Templates     = lazy(() => import('../features/templates/Templates'));
const TemplateCreator = lazy(() => import('../features/templates/TemplateCreator'));
const ProjectSettings = lazy(() => import('../features/settings/ProjectSettings'));
const GlobalSettings  = lazy(() => import('../features/settings/GlobalSettings'));
const Fleet         = lazy(() => import('../features/fleet/FleetPage'));

export const routes: RouteDefinition[] = [
  { path: '/',         component: Projects },
  { path: '/projects', component: Projects },
  { path: '/templates',     component: Templates },
  { path: '/templates/new', component: TemplateCreator },
  { path: '/fleet',    component: Fleet },
  { path: '/settings', component: GlobalSettings },

  // Project destinations
  { path: '/projects/:id/overview',  component: Dashboard },
  { path: '/projects/:id/settings',  component: ProjectSettings },

  // Work surface — all 5 lenses wrapped in WorkLayout
  {
    path: '/projects/:id',
    component: WorkLayout,
    children: [
      { path: '/board',    component: Board },
      { path: '/list',     component: List },
      { path: '/table',    component: Table },
      { path: '/calendar', component: Calendar },
      { path: '/timeline', component: Timeline },
      { path: '/sprint',   component: Sprints },
    ],
  },

  // Catch-all 404
  { path: '*', component: NotFound },
];

function NotFound() {
  const location = useLocation();
  return (
    <div class="flex flex-col items-center justify-center py-24 text-center">
      <div class="mb-4 text-7xl" aria-hidden="true">🔍</div>
      <h1 class="text-2xl font-bold mb-2" style={{ color: 'var(--color-text-primary)' }}>
        Page not found
      </h1>
      <p class="text-sm mb-6 max-w-xs" style={{ color: 'var(--color-text-secondary)' }}>
        <code class="font-mono">{location.pathname}</code> doesn't exist.
      </p>
      <Button onClick={() => history.back()}>Go back</Button>
    </div>
  );
}
