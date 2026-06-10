import { lazy } from 'solid-js';
import { Navigate } from '@solidjs/router';
import type { RouteDefinition } from '@solidjs/router';
import WorkLayout from './WorkLayout';

const Projects      = lazy(() => import('../features/projects/Projects'));
const Board         = lazy(() => import('../features/board/Board'));
const List          = lazy(() => import('../features/list/List'));
const Dashboard     = lazy(() => import('../features/dashboard/Dashboard'));
const Sprints       = lazy(() => import('../features/sprints/Sprints'));
const Calendar      = lazy(() => import('../features/calendar/Calendar'));
const Timeline      = lazy(() => import('../features/timeline/Timeline'));
const TreeView      = lazy(() => import('../features/tree/TreeView'));
const Templates     = lazy(() => import('../features/templates/Templates'));
const TemplateCreator = lazy(() => import('../features/templates/TemplateCreator'));
const ProjectSettings = lazy(() => import('../features/settings/ProjectSettings'));
const GlobalSettings  = lazy(() => import('../features/settings/GlobalSettings'));

export const routes: RouteDefinition[] = [
  { path: '/',         component: Projects },
  { path: '/projects', component: Projects },
  { path: '/templates',     component: Templates },
  { path: '/templates/new', component: TemplateCreator },
  { path: '/settings', component: GlobalSettings },

  // Project destinations
  { path: '/projects/:id/overview',  component: Dashboard },
  { path: '/projects/:id/sprints',   component: Sprints },
  { path: '/projects/:id/settings',  component: ProjectSettings },

  // Work surface — 5 lenses wrapped in WorkLayout
  {
    path: '/projects/:id',
    component: WorkLayout,
    children: [
      { path: '/board',    component: Board },
      { path: '/board/:boardId', component: Board },
      { path: '/list',     component: List },
      { path: '/tree',     component: TreeView },
      { path: '/calendar', component: Calendar },
      { path: '/timeline', component: Timeline },
    ],
  },

  // Legacy bare routes → home (no more dead ends)
  { path: '/board',  component: () => <Navigate href="/" /> },
  { path: '/list',   component: () => <Navigate href="/" /> },
];
