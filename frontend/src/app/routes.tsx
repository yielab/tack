import { lazy } from 'solid-js';
import type { RouteDefinition } from '@solidjs/router';

const Projects = lazy(() => import('../features/projects/Projects'));
const Board = lazy(() => import('../features/board/Board'));
const List = lazy(() => import('../features/list/List'));
const Dashboard = lazy(() => import('../features/dashboard/Dashboard'));
const Sprints = lazy(() => import('../features/sprints/Sprints'));
const Calendar = lazy(() => import('../features/calendar/Calendar'));
const Timeline = lazy(() => import('../features/timeline/Timeline'));
const BoardsManager = lazy(() => import('../features/settings/BoardsManager'));
const CustomFieldsManager = lazy(
  () => import('../features/settings/CustomFieldsManager')
);
const Templates = lazy(() => import('../features/templates/Templates'));
const TemplateCreator = lazy(() => import('../features/templates/TemplateCreator'));
const Settings = lazy(() => import('../features/settings/Settings'));
const GlobalSettings = lazy(() => import('../features/settings/GlobalSettings'));

export const routes: RouteDefinition[] = [
  { path: '/', component: Projects },
  { path: '/projects', component: Projects },
  { path: '/projects/:id/board', component: Board },
  { path: '/projects/:id/board/:boardId', component: Board },
  { path: '/projects/:id/list', component: List },
  { path: '/projects/:id/dashboard', component: Dashboard },
  { path: '/projects/:id/sprints', component: Sprints },
  { path: '/projects/:id/calendar', component: Calendar },
  { path: '/projects/:id/timeline', component: Timeline },
  { path: '/projects/:id/settings', component: Settings },
  { path: '/projects/:id/settings/boards', component: BoardsManager },
  { path: '/projects/:id/settings/fields', component: CustomFieldsManager },
  { path: '/templates', component: Templates },
  { path: '/templates/new', component: TemplateCreator },
  { path: '/board', component: Board },
  { path: '/list', component: List },
  { path: '/settings', component: GlobalSettings },
];
