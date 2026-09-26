import { lazy } from 'solid-js';
import { useLocation } from '@solidjs/router';
import type { RouteDefinition } from '@solidjs/router';
import { Button, BrandMark } from '../shared/ui';
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
const Agents        = lazy(() => import('../features/agents/AgentsPage'));

export const routes: RouteDefinition[] = [
  { path: '/',         component: Projects },
  { path: '/projects', component: Projects },
  { path: '/templates',     component: Templates },
  { path: '/templates/new', component: TemplateCreator },
  { path: '/agents',     component: Agents },
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
    <div class="flex flex-col items-start gap-3.5 py-14 lg:px-6">
      <BrandMark size={110} />
      <h1 class="text-[34px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
        Page not found
      </h1>
      <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
        <code
          class="font-mono rounded-lg px-2 py-0.5"
          style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)' }}
        >
          {location.pathname}
        </code>{' '}doesn't exist.
      </p>
      <Button onClick={() => history.back()}>Go back</Button>
    </div>
  );
}
