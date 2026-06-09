import { Router } from '@solidjs/router';
import Layout from './Layout';
import { routes } from './routes';

function App() {
  return <Router root={Layout}>{routes}</Router>;
}

export default App;
