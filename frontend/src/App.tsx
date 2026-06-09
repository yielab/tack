import { lazy } from 'solid-js';
import { Router, Route } from '@solidjs/router';
import Layout from './components/Layout';

const Projects = lazy(() => import('./pages/Projects'));
const Board = lazy(() => import('./pages/Board'));
const List = lazy(() => import('./pages/List'));
const Dashboard = lazy(() => import('./pages/Dashboard'));
const Sprints = lazy(() => import('./pages/Sprints'));
const Calendar = lazy(() => import('./pages/Calendar'));
const Timeline = lazy(() => import('./pages/Timeline'));
const BoardsManager = lazy(() => import('./pages/BoardsManager'));
const CustomFieldsManager = lazy(() => import('./pages/CustomFieldsManager'));
const Templates = lazy(() => import('./pages/Templates'));
const TemplateCreator = lazy(() => import('./pages/TemplateCreator'));
const Settings = lazy(() => import('./pages/Settings'));

function App() {
  return (
    <Router root={Layout}>
      <Route path="/" component={Projects} />
      <Route path="/projects" component={Projects} />
      <Route path="/projects/:id/board" component={Board} />
      <Route path="/projects/:id/board/:boardId" component={Board} />
      <Route path="/projects/:id/list" component={List} />
      <Route path="/projects/:id/dashboard" component={Dashboard} />
      <Route path="/projects/:id/sprints" component={Sprints} />
      <Route path="/projects/:id/calendar" component={Calendar} />
      <Route path="/projects/:id/timeline" component={Timeline} />
      <Route path="/projects/:id/settings" component={Settings} />
      <Route path="/projects/:id/settings/boards" component={BoardsManager} />
      <Route path="/projects/:id/settings/fields" component={CustomFieldsManager} />
      <Route path="/templates" component={Templates} />
      <Route path="/templates/new" component={TemplateCreator} />
      <Route path="/board" component={Board} />
      <Route path="/list" component={List} />
      <Route path="/settings" component={Settings} />
    </Router>
  );
}

export default App;
