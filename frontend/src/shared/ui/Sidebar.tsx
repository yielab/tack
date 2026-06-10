import { A, useParams, useLocation, useNavigate } from '@solidjs/router';
import { FiHome, FiSettings, FiMenu, FiX, FiLayers, FiGitBranch, FiBarChart2, FiBookOpen } from 'solid-icons/fi';
import { createSignal, Show, For, createResource, type Component } from 'solid-js';
import { api } from '../api';
import { getLastLens } from '../state/lastView';

const NavLink: Component<{ href: string; icon: any; label: string; end?: boolean; onClick?: () => void }> = (p) => {
  const location = useLocation();
  const active = () => p.end ? location.pathname === p.href : location.pathname.startsWith(p.href);

  return (
    <A
      href={p.href}
      class="flex items-center px-3 py-2.5 text-sm font-medium rounded-lg transition-all"
      style={active()
        ? { background: 'var(--color-primary-50)', color: 'var(--color-primary-600)' }
        : { color: 'var(--color-text-secondary)' }}
      onClick={p.onClick}
    >
      <p.icon class="mr-3 shrink-0" size={17} />
      {p.label}
    </A>
  );
};

const SectionLabel: Component<{ label: string }> = (p) => (
  <p class="px-3 pb-1.5 text-xs font-bold uppercase tracking-widest" style={{ color: 'var(--color-text-tertiary)' }}>
    {p.label}
  </p>
);

const Sidebar: Component = () => {
  const params = useParams();
  const location = useLocation();
  const navigate = useNavigate();
  const [isOpen, setIsOpen] = createSignal(false);
  const close = () => setIsOpen(false);

  const currentProjectId = () => params.id as string | undefined;
  const [projects] = createResource(() => api.projects.list());

  // "Work" is active when on any lens route
  const workActive = () => {
    const p = location.pathname;
    return ['board', 'list', 'tree', 'calendar', 'timeline'].some(l =>
      p.endsWith(`/${l}`) || p.includes(`/${l}/`)
    );
  };

  const handleProjectSwitch = (id: string) => {
    const lens = getLastLens();
    navigate(`/projects/${id}/${lens}`);
    close();
  };

  const inner = (
    <div class="flex flex-col h-full">
      {/* Logo */}
      <div class="hidden lg:flex items-center px-5 py-4 border-b" style={{ 'border-color': 'var(--color-border-light)' }}>
        <A href="/projects" class="flex items-center gap-2" onClick={close}>
          <div class="w-7 h-7 bg-linear-to-br from-violet-500 to-purple-600 rounded-lg flex items-center justify-center">
            <span class="text-white font-bold text-sm">F</span>
          </div>
          <span class="text-lg font-bold" style={{ color: 'var(--color-text-primary)' }}>FlexPM</span>
        </A>
      </div>

      <nav class="flex-1 px-3 py-4 space-y-6 overflow-y-auto mt-14 lg:mt-0">
        {/* Workspace */}
        <div class="space-y-0.5">
          <SectionLabel label="Workspace" />
          <NavLink href="/projects" end icon={FiHome}     label="All Projects" onClick={close} />
          <NavLink href="/templates"     icon={FiBookOpen} label="Templates"    onClick={close} />
          <NavLink href="/settings"      icon={FiSettings} label="Settings"     onClick={close} />
        </div>

        {/* Current project */}
        <Show when={currentProjectId()}>
          <div class="space-y-1">
            <SectionLabel label="Project" />

            {/* Instant project switcher — no page reload */}
            <div class="px-1 pb-1">
              <select
                value={currentProjectId()}
                onChange={(e) => handleProjectSwitch(e.currentTarget.value)}
                class="w-full px-3 py-2 text-sm font-semibold rounded-lg border cursor-pointer transition-all focus:outline-none focus-visible:ring-2"
                style={{
                  background: 'var(--color-bg-subtle)',
                  color: 'var(--color-text-primary)',
                  'border-color': 'var(--color-border-medium)',
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              >
                <For each={projects()}>
                  {(p) => <option value={p.id}>{p.name}</option>}
                </For>
              </select>
            </div>

            {/* Overview */}
            <NavLink
              href={`/projects/${currentProjectId()}/overview`}
              icon={FiBarChart2}
              label="Overview"
              onClick={close}
            />

            {/* Work (all 5 lenses behind one destination) */}
            <A
              href={`/projects/${currentProjectId()}/${getLastLens()}`}
              class="flex items-center px-3 py-2.5 text-sm font-medium rounded-lg transition-all"
              style={workActive()
                ? { background: 'var(--color-primary-50)', color: 'var(--color-primary-600)' }
                : { color: 'var(--color-text-secondary)' }}
              onClick={close}
            >
              <FiLayers class="mr-3 shrink-0" size={17} />
              Work
            </A>

            <NavLink
              href={`/projects/${currentProjectId()}/sprint`}
              icon={FiGitBranch}
              label="Sprint"
              onClick={close}
            />
            <NavLink
              href={`/projects/${currentProjectId()}/settings`}
              icon={FiSettings}
              label="Settings"
              onClick={close}
            />
          </div>
        </Show>

        {/* Other projects quick-links */}
        <Show when={projects() && (projects()!.length > 1 || !currentProjectId())}>
          <div class="space-y-0.5">
            <SectionLabel label={currentProjectId() ? 'Other Projects' : 'Projects'} />
            <For each={projects()}>
              {(p) => (
                <Show when={p.id !== currentProjectId()}>
                  <button
                    class="w-full flex items-start px-3 py-2 text-sm rounded-lg text-left transition-all"
                    style={{ color: 'var(--color-text-secondary)' }}
                    onClick={() => { handleProjectSwitch(p.id); }}
                    onMouseEnter={(e) => { e.currentTarget.style.background = 'var(--color-bg-hover)'; e.currentTarget.style.color = 'var(--color-text-primary)'; }}
                    onMouseLeave={(e) => { e.currentTarget.style.background = ''; e.currentTarget.style.color = 'var(--color-text-secondary)'; }}
                  >
                    <span class="truncate">{p.name}</span>
                  </button>
                </Show>
              )}
            </For>
          </div>
        </Show>
      </nav>

      <div class="px-4 py-3 border-t" style={{ 'border-color': 'var(--color-border-light)' }}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>FlexPM v1.0</p>
      </div>
    </div>
  );

  return (
    <>
      {/* Mobile top bar */}
      <div
        class="lg:hidden fixed top-0 left-0 right-0 z-20 border-b shadow-sm flex items-center justify-between px-4 py-3"
        style={{ background: 'var(--color-bg-elevated)', 'border-color': 'var(--color-border-light)' }}
      >
        <span class="text-xl font-bold" style={{ color: 'var(--color-text-primary)' }}>FlexPM</span>
        <button onClick={() => setIsOpen(!isOpen())} class="p-2 rounded-lg transition-colors" style={{ color: 'var(--color-text-secondary)' }}>
          <Show when={isOpen()} fallback={<FiMenu size={22} />}><FiX size={22} /></Show>
        </button>
      </div>

      {/* Sidebar panel */}
      <div
        class={`fixed inset-y-0 left-0 z-10 w-60 border-r transform transition-transform duration-200 ease-in-out lg:translate-x-0 lg:static ${isOpen() ? 'translate-x-0' : '-translate-x-full'}`}
        style={{ background: 'var(--color-bg-sidebar)', 'border-color': 'var(--color-border-medium)' }}
      >
        {inner}
      </div>

      {/* Mobile overlay */}
      <Show when={isOpen()}>
        <div class="fixed inset-0 bg-black/40 z-0 lg:hidden" onClick={close} />
      </Show>
    </>
  );
};

export default Sidebar;
