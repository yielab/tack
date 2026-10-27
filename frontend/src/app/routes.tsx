import { lazy, type Component } from 'solid-js';
import { useLocation } from '@solidjs/router';
import type { RouteDefinition } from '@solidjs/router';
import { Button, BrandMark, RouteBoundary } from '../shared/ui';
import WorkLayout from './WorkLayout';

const Projects      = lazy(() => import('../features/projects/Projects'));
const Board         = lazy(() => import('../features/board/Board'));
const List          = lazy(() => import('../features/list/List'));
const Table         = lazy(() => import('../features/table/Table'));
const Dashboard     = lazy(() => import('../features/dashboard/Dashboard'));
const FactoryMetrics = lazy(() => import('../features/dashboard/FactoryMetrics'));
const Sprints       = lazy(() => import('../features/sprints/Sprints'));
const Calendar      = lazy(() => import('../features/calendar/Calendar'));
const Timeline      = lazy(() => import('../features/timeline/Timeline'));
const Templates     = lazy(() => import('../features/templates/Templates'));
const TemplateCreator = lazy(() => import('../features/templates/TemplateCreator'));
const ProjectSettings = lazy(() => import('../features/settings/ProjectSettings'));
const GlobalSettings  = lazy(() => import('../features/settings/GlobalSettings'));
const ItemPage      = lazy(() => import('../features/item-detail/ItemPage'));
const Agents        = lazy(() => import('../features/agents/AgentsPage'));

/** Each route draws inside its own boundary, so one that fails leaves the shell and the rest of the app standing. */
const guarded = (Page: Component): Component => (props) => (
  <RouteBoundary>
    <Page {...props} />
  </RouteBoundary>
);

export const routes: RouteDefinition[] = [
  { path: '/',         component: guarded(Projects) },
  { path: '/projects', component: guarded(Projects) },
  { path: '/templates',     component: guarded(Templates) },
  { path: '/templates/new', component: guarded(TemplateCreator) },
  { path: '/agents',     component: guarded(Agents) },
  { path: '/settings', component: guarded(GlobalSettings) },

  // Project destinations
  { path: '/projects/:id/overview',   component: guarded(Dashboard) },
  { path: '/projects/:id/factory',    component: guarded(FactoryMetrics) },
  { path: '/projects/:id/settings',   component: guarded(ProjectSettings) },

  // An item on a page of its own (the drawer's "Expand")
  { path: '/projects/:id/items/:itemId', component: guarded(ItemPage) },

  // Work surface — all 5 lenses wrapped in WorkLayout
  {
    path: '/projects/:id',
    component: guarded(WorkLayout),
    children: [
      { path: '/board',    component: guarded(Board) },
      { path: '/list',     component: guarded(List) },
      { path: '/table',    component: guarded(Table) },
      { path: '/calendar', component: guarded(Calendar) },
      { path: '/timeline', component: guarded(Timeline) },
      { path: '/sprint',   component: guarded(Sprints) },
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
      <h1 class="text-[2.125rem] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
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
