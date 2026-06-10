import { type Component, type JSX, lazy, Show, createSignal, createEffect, onMount, onCleanup } from 'solid-js';
import { useSearchParams, useNavigate, useParams, useLocation } from '@solidjs/router';
import Sidebar from '../shared/ui/Sidebar';
import SearchBar from '../shared/ui/SearchBar';
import ToastContainer from '../shared/ui/ToastContainer';
import Breadcrumb from '../shared/ui/Breadcrumb';
import { ProjectProvider } from '../shared/state/projectContext';
import CommandPalette, { type Command } from '../shared/ui/CommandPalette';
import { Button } from '../shared/ui';
import { getLastLens } from '../shared/state/lastView';
import CreateItemModal from '../shared/ui/CreateItemModal';
import CreateProjectModal from '../features/projects/CreateProjectModal';
import { useProject } from '../shared/state/projectContext';

const ItemDetailDrawer = lazy(() => import('../features/item-detail/ItemDetailDrawer'));

interface LayoutProps {
  children?: JSX.Element;
}

const VIEW_LABELS: Record<string, string> = {
  board: 'Board', list: 'List', tree: 'Tree', calendar: 'Calendar', timeline: 'Timeline',
  overview: 'Overview', sprints: 'Sprints', settings: 'Settings',
};

/** Inner layout — needs router context, so lives inside ProjectProvider. */
const LayoutInner: Component<LayoutProps> = (props) => {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const params = useParams();
  const location = useLocation();
  const { project } = useProject();

  const projectId = () => params.id as string | undefined;

  // document.title — "<Project> — <View> · FlexPM"
  createEffect(() => {
    const path = location.pathname;
    const segments = path.split('/').filter(Boolean);
    const view = [...Object.keys(VIEW_LABELS)].find(v => segments.includes(v));
    const proj = project();

    let title = 'FlexPM';
    if (proj && view) {
      title = `${proj.name} — ${VIEW_LABELS[view]} · FlexPM`;
    } else if (proj) {
      title = `${proj.name} · FlexPM`;
    } else if (path.startsWith('/templates')) {
      title = 'Templates · FlexPM';
    } else if (path.startsWith('/settings')) {
      title = 'Settings · FlexPM';
    } else {
      title = 'Projects · FlexPM';
    }
    document.title = title;
  });

  // Global command palette
  const [showPalette, setShowPalette] = createSignal(false);

  // Context-aware + New
  const [showNewItem, setShowNewItem] = createSignal(false);
  const [showNewProject, setShowNewProject] = createSignal(false);

  // Ctrl+K global shortcut
  onMount(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
        e.preventDefault();
        setShowPalette(true);
      }
    };
    window.addEventListener('keydown', handler);
    onCleanup(() => window.removeEventListener('keydown', handler));
  });

  const globalCommands = (): Command[] => {
    const pid = projectId();
    const lens = getLastLens();
    const cmds: Command[] = [];

    if (pid) {
      cmds.push(
        { id: 'new-item',     label: 'New Item',          icon: '➕', shortcut: 'N', action: () => setShowNewItem(true) },
        { id: 'go-board',     label: 'Work → Board',      icon: '⬛', action: () => navigate(`/projects/${pid}/board`) },
        { id: 'go-list',      label: 'Work → List',       icon: '☰',  action: () => navigate(`/projects/${pid}/list`) },
        { id: 'go-tree',      label: 'Work → Tree',       icon: '🌲', action: () => navigate(`/projects/${pid}/tree`) },
        { id: 'go-calendar',  label: 'Work → Calendar',   icon: '📅', action: () => navigate(`/projects/${pid}/calendar`) },
        { id: 'go-timeline',  label: 'Work → Timeline',   icon: '📊', action: () => navigate(`/projects/${pid}/timeline`) },
        { id: 'go-overview',  label: 'Overview',          icon: '📈', action: () => navigate(`/projects/${pid}/overview`) },
        { id: 'go-sprints',   label: 'Sprints',           icon: '🏃', action: () => navigate(`/projects/${pid}/sprints`) },
        { id: 'go-settings',  label: 'Project Settings',  icon: '⚙️', action: () => navigate(`/projects/${pid}/settings`) },
      );
      void lens; // used via getLastLens in sidebar
    }

    cmds.push(
      { id: 'go-projects',  label: 'All Projects',        icon: '🏠', action: () => navigate('/projects') },
      { id: 'go-templates', label: 'Templates',           icon: '📋', action: () => navigate('/templates') },
      { id: 'go-gsettings', label: 'Global Settings',     icon: '🔧', action: () => navigate('/settings') },
      { id: 'new-project',  label: 'New Project',         icon: '📁', action: () => setShowNewProject(true) },
    );

    return cmds;
  };

  return (
    <div class="flex h-screen" style={{ 'background-color': 'var(--color-bg-app)' }}>
      <Sidebar />

      <main class="flex-1 overflow-auto pt-14 lg:pt-0 flex flex-col min-w-0">
        {/* Top bar */}
        <div
          class="sticky top-0 z-40 border-b px-4 py-2.5 shadow-sm"
          style={{
            'background-color': 'var(--color-bg-base)',
            'border-color': 'var(--color-border-light)',
          }}
        >
          <div class="flex items-center justify-between gap-4 max-w-7xl mx-auto">
            <Breadcrumb />

            <div class="flex items-center gap-2">
              <SearchBar placeholder="Search… (Ctrl+/)" />

              {/* ＋ New — context-aware */}
              <Button
                size="sm"
                onClick={() => projectId() ? setShowNewItem(true) : setShowNewProject(true)}
                title={projectId() ? 'New item (Ctrl+K → New Item)' : 'New project'}
              >
                + New
              </Button>

              {/* ⌘K trigger */}
              <button
                onClick={() => setShowPalette(true)}
                class="hidden sm:flex items-center gap-1 px-2.5 py-1.5 text-xs border rounded-md transition-colors"
                style={{
                  color: 'var(--color-text-tertiary)',
                  'border-color': 'var(--color-border-medium)',
                  background: 'var(--color-bg-subtle)',
                }}
              >
                <span class="font-mono">Ctrl+K</span>
              </button>
            </div>
          </div>
        </div>

        <div class="flex-1 container mx-auto px-4 py-6 max-w-7xl">
          {props.children}
        </div>
      </main>

      {/* Global command palette */}
      <CommandPalette
        isOpen={showPalette()}
        onClose={() => setShowPalette(false)}
        commands={globalCommands()}
      />

      {/* Context-aware create modals */}
      <Show when={showNewItem() && projectId()}>
        <CreateItemModal
          isOpen={showNewItem()}
          onClose={() => setShowNewItem(false)}
          onSuccess={() => setShowNewItem(false)}
          projectId={projectId()!}
        />
      </Show>
      <Show when={showNewProject()}>
        <CreateProjectModal
          isOpen={showNewProject()}
          onClose={() => setShowNewProject(false)}
          onSuccess={() => setShowNewProject(false)}
        />
      </Show>

      {/* Item detail drawer */}
      <Show when={searchParams.item}>
        <ItemDetailDrawer />
      </Show>

      <ToastContainer />
    </div>
  );
};

const Layout: Component<LayoutProps> = (props) => (
  <ProjectProvider>
    <LayoutInner>{props.children}</LayoutInner>
  </ProjectProvider>
);

export default Layout;
