import { Router, Route } from '@solidjs/router';
import Layout from './components/Layout';
import Projects from './pages/Projects';
import Board from './pages/Board';
import List from './pages/List';
import Dashboard from './pages/Dashboard';
import Sprints from './pages/Sprints';
import Calendar from './pages/Calendar';
import Timeline from './pages/Timeline';
import BoardsManager from './pages/BoardsManager';
import CustomFieldsManager from './pages/CustomFieldsManager';
import Templates from './pages/Templates';
import TemplateCreator from './pages/TemplateCreator';
import Settings from './pages/Settings';

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
