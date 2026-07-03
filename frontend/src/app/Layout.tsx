import { type Component, type JSX, lazy, Show, createSignal, createEffect, onMount, onCleanup } from 'solid-js';
import { useSearchParams, useNavigate, useParams, useLocation } from '@solidjs/router';
import Sidebar from '../shared/ui/Sidebar';
import SearchBar from '../shared/ui/SearchBar';
import ToastContainer from '../shared/ui/ToastContainer';
import Breadcrumb from '../shared/ui/Breadcrumb';
import { ProjectProvider } from '../shared/state/projectContext';
import CommandPalette, { type Command } from '../shared/ui/CommandPalette';
import { paletteOpen, openPalette, closePalette } from '../shared/state/commandPalette';
import { IconPlus } from '../shared/ui/icons';
import { getLastLens } from '../shared/state/lastView';
import CreateItemModal from '../shared/ui/CreateItemModal';
import CreateProjectModal from '../features/projects/CreateProjectModal';
import { useProject } from '../shared/state/projectContext';
import { useVocab } from '../shared/vocab/useVocab';

const ItemDetailDrawer = lazy(() => import('../features/item-detail/ItemDetailDrawer'));

interface LayoutProps {
  children?: JSX.Element;
}

const VIEW_LABELS: Record<string, string> = {
  board: 'Board', list: 'List', table: 'Table', calendar: 'Calendar', timeline: 'Timeline', sprint: 'Sprint',
  overview: 'Overview', settings: 'Settings',
};

// Views whose document-title label comes from the project vocabulary.
const VOCAB_VIEW_KEYS: Record<string, string> = { sprint: 'sprint' };

/** Inner layout — needs router context, so lives inside ProjectProvider. */
const LayoutInner: Component<LayoutProps> = (props) => {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const params = useParams();
  const location = useLocation();
  const { project, vocabulary } = useProject();
  const { t } = useVocab();

  const projectId = () => params.id as string | undefined;

  // The board manages its own full-height horizontal-scroll layout.
  const fullBleed = () => location.pathname.endsWith('/board');

  // document.title — "<Project> — <View> · Tack"
  createEffect(() => {
    const path = location.pathname;
    const segments = path.split('/').filter(Boolean);
    const view = [...Object.keys(VIEW_LABELS)].find(v => segments.includes(v));
    const proj = project();

    let title = 'Tack';
    if (proj && view) {
      const viewLabel = VOCAB_VIEW_KEYS[view] ? t(VOCAB_VIEW_KEYS[view]) : VIEW_LABELS[view];
      title = `${proj.name} — ${viewLabel} · Tack`;
    } else if (proj) {
      title = `${proj.name} · Tack`;
    } else if (path.startsWith('/templates')) {
      title = 'Templates · Tack';
    } else if (path.startsWith('/settings')) {
      title = 'Settings · Tack';
    } else {
      title = 'Projects · Tack';
    }
    document.title = title;
  });

  // Context-aware + New
  const [showNewItem, setShowNewItem] = createSignal(false);
  const [showNewProject, setShowNewProject] = createSignal(false);

  // Ctrl+K opens the command palette (Ctrl+/ is owned by SearchBar → focus).
  onMount(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
        e.preventDefault();
        openPalette();
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
        { id: 'new-item',     label: 'New Item',          icon: '➕', shortcut: 'N', group: 'Actions', action: () => setShowNewItem(true) },
        { id: 'go-board',     label: 'Work → Board',      icon: '⬛', group: 'Go to', action: () => navigate(`/projects/${pid}/board`) },
        { id: 'go-list',      label: 'Work → List',       icon: '☰',  group: 'Go to', action: () => navigate(`/projects/${pid}/list`) },
        { id: 'go-table',     label: 'Work → Table',      icon: '▦',  group: 'Go to', action: () => navigate(`/projects/${pid}/table`) },
        { id: 'go-calendar',  label: 'Work → Calendar',   icon: '📅', group: 'Go to', action: () => navigate(`/projects/${pid}/calendar`) },
        { id: 'go-timeline',  label: 'Work → Timeline',   icon: '📊', group: 'Go to', action: () => navigate(`/projects/${pid}/timeline`) },
        { id: 'go-sprint',    label: `Work → ${t('sprint')}`, icon: '🏃', group: 'Go to', action: () => navigate(`/projects/${pid}/sprint`) },
        { id: 'go-overview',  label: 'Overview',          icon: '📈', group: 'Go to', action: () => navigate(`/projects/${pid}/overview`) },
        { id: 'go-settings',  label: 'Project Settings',  icon: '⚙️', group: 'Go to', action: () => navigate(`/projects/${pid}/settings`) },
      );
      void lens; // used via getLastLens in sidebar
    }

    cmds.push(
      { id: 'go-projects',  label: 'All Projects',        icon: '🏠', group: 'Workspace', action: () => navigate('/projects') },
      { id: 'go-templates', label: 'Templates',           icon: '📋', group: 'Workspace', action: () => navigate('/templates') },
      { id: 'go-gsettings', label: 'Global Settings',     icon: '🔧', group: 'Workspace', action: () => navigate('/settings') },
      { id: 'new-project',  label: 'New Project',         icon: '📁', group: 'Actions', action: () => setShowNewProject(true) },
    );

    return cmds;
  };

  return (
    <div class="flex h-screen" style={{ 'background-color': 'var(--color-bg-app)' }}>
      <Sidebar />

      <main class="flex-1 overflow-hidden pt-14 lg:pt-0 flex flex-col min-w-0">
        {/* Top bar */}
        <div
          class="sticky top-0 z-40 flex items-center gap-3.5"
          style={{
            height: '54px',
            'flex-shrink': 0,
            padding: '0 18px',
            'background-color': 'var(--color-bg-base)',
            'border-bottom': '1px solid var(--color-border-light)',
          }}
        >
          <Breadcrumb />

          <div style={{ flex: 1 }} />

          {/* item search */}
          <div class="hidden sm:block">
            <SearchBar projectId={projectId()} />
          </div>

          {/* ＋ New — context-aware */}
          <button
            onClick={() => projectId() ? setShowNewItem(true) : setShowNewProject(true)}
            title={projectId() ? 'New item' : 'New project'}
            style={{
              display: 'flex', 'align-items': 'center', gap: '6px',
              padding: '8px 13px', 'border-radius': '9px', border: 'none', cursor: 'pointer',
              background: 'var(--color-primary-600)', color: 'var(--color-on-accent)',
              'font-family': 'inherit', 'font-size': '12.5px', 'font-weight': 700,
              'box-shadow': 'var(--shadow-sm)',
            }}
          >
            <IconPlus size={14} /> New
          </button>

          {/* ⌃K trigger */}
          <button
            onClick={() => openPalette()}
            title="Command palette"
            style={{
              display: 'flex', 'align-items': 'center', padding: '7px 9px',
              'border-radius': '8px', cursor: 'pointer',
              border: '1px solid var(--color-border-light)', background: 'var(--color-bg-app)',
              color: 'var(--color-text-tertiary)', 'font-family': 'var(--font-mono)', 'font-size': '11px',
            }}
          >
            ⌃K
          </button>
        </div>

        {/* Board is full-bleed (own scroll); other views get the centered column. */}
        <Show
          when={fullBleed()}
          fallback={<div class="flex-1 overflow-auto container mx-auto px-4 py-6 max-w-7xl">{props.children}</div>}
        >
          <div class="flex-1 min-w-0 flex flex-col overflow-hidden">{props.children}</div>
        </Show>
      </main>

      {/* Global command palette */}
      <CommandPalette
        isOpen={paletteOpen()}
        onClose={closePalette}
        commands={globalCommands()}
      />

      {/* Context-aware create modals */}
      <Show when={showNewItem() && projectId()}>
        <CreateItemModal
          isOpen={showNewItem()}
          onClose={() => setShowNewItem(false)}
          onSuccess={() => setShowNewItem(false)}
          projectId={projectId()!}
          vocabulary={vocabulary()}
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
