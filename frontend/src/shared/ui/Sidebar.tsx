import { A, useParams, useLocation } from '@solidjs/router';
import { FiHome, FiGrid, FiList, FiSettings, FiMenu, FiX, FiBarChart2, FiCalendar, FiGitBranch } from 'solid-icons/fi';
import { createSignal, Show, For, createResource, type Component, createMemo } from 'solid-js';
import { api } from '../api';

const Sidebar: Component = () => {
  const params = useParams();
  const location = useLocation();
  const currentProjectId = () => params.id;

  const [isOpen, setIsOpen] = createSignal(false);
  const [projects] = createResource(() => api.projects.list());

  // Determine current view
  const currentView = createMemo(() => {
    const path = location.pathname;
    if (path.includes('/settings')) return 'settings';
    if (path.includes('/board')) return 'board';
    if (path.includes('/list')) return 'list';
    if (path.includes('/dashboard')) return 'dashboard';
    if (path.includes('/sprints')) return 'sprints';
    if (path.includes('/calendar')) return 'calendar';
    if (path.includes('/timeline')) return 'timeline';
    return null;
  });

  return (
    <>
      {/* Mobile menu button */}
      <div class="lg:hidden fixed top-0 left-0 right-0 bg-[var(--color-bg-elevated)] border-b border-[var(--color-border-light)] z-20 shadow-sm">
        <div class="flex items-center justify-between px-4 py-3">
          <h1 class="text-xl font-bold text-[var(--color-text-primary)]">FlexPM</h1>
          <button
            onClick={() => setIsOpen(!isOpen())}
            class="p-2 rounded-lg text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors"
          >
            <Show when={isOpen()} fallback={<FiMenu size={24} />}>
              <FiX size={24} />
            </Show>
          </button>
        </div>
      </div>

      {/* Sidebar */}
      <div
        class={`fixed inset-y-0 left-0 z-10 w-64 border-r transform transition-transform duration-200 ease-in-out lg:translate-x-0 lg:static ${isOpen() ? 'translate-x-0' : '-translate-x-full'}`}
        style={{
          "background-color": "var(--color-bg-sidebar)",
          "border-color": "var(--color-border-medium)"
        }}
      >
        <div class="flex flex-col h-full">
          {/* Logo */}
          <div class="hidden lg:flex items-center px-6 py-5 border-b border-[var(--color-border-light)]">
            <div class="flex items-center gap-2">
              <div class="w-8 h-8 bg-gradient-to-br from-violet-500 to-purple-600 rounded-lg flex items-center justify-center">
                <span class="text-white font-bold text-lg">F</span>
              </div>
              <h1 class="text-xl font-bold text-[var(--color-text-primary)]">FlexPM</h1>
            </div>
          </div>

          {/* Navigation */}
          <nav class="flex-1 px-3 py-4 space-y-6 overflow-y-auto mt-14 lg:mt-0">
            {/* Home / All Projects */}
            <div>
              <A
                href="/"
                end
                activeClass="bg-violet-50 dark:bg-violet-500/10 text-violet-600 dark:text-violet-400 font-semibold"
                inactiveClass="text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                class="flex items-center px-3 py-2.5 text-sm font-medium rounded-lg transition-all"
                onClick={() => setIsOpen(false)}
              >
                <FiHome class="mr-3" size={18} />
                All Projects
              </A>
            </div>

            {/* Current Project Section */}
            <Show when={currentProjectId()}>
              <div>
                <div class="px-3 pb-2">
                  <p class="text-xs font-bold text-[var(--color-text-tertiary)] uppercase tracking-wider">
                    Current Project
                  </p>
                </div>

                {/* Project Selector */}
                <div class="px-3 pb-3">
                  <select
                    value={currentProjectId()}
                    onChange={(e) => {
                      const newProjectId = e.currentTarget.value;
                      window.location.href = `/projects/${newProjectId}/${currentView() || 'board'}`;
                    }}
                    class="w-full px-3 py-2.5 text-sm font-semibold bg-[var(--color-bg-subtle)] border border-[var(--color-border-medium)] rounded-lg text-[var(--color-text-primary)] focus:ring-2 focus:ring-violet-500 focus:border-violet-500 transition-all cursor-pointer hover:border-[var(--color-border-strong)]"
                  >
                    <For each={projects()}>
                      {(project) => (
                        <option value={project.id}>{project.name}</option>
                      )}
                    </For>
                  </select>
                </div>

                {/* Project Views */}
                <div class="space-y-0.5">
                  <A
                    href={`/projects/${currentProjectId()}/board`}
                    activeClass="bg-violet-50 dark:bg-violet-500/10 text-violet-600 dark:text-violet-400 font-semibold"
                    inactiveClass="text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                    class="flex items-center px-3 py-2.5 text-sm font-medium rounded-lg transition-all"
                    onClick={() => setIsOpen(false)}
                  >
                    <FiGrid class="mr-3" size={18} />
                    Board
                  </A>
                  <A
                    href={`/projects/${currentProjectId()}/list`}
                    activeClass="bg-violet-50 dark:bg-violet-500/10 text-violet-600 dark:text-violet-400 font-semibold"
                    inactiveClass="text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                    class="flex items-center px-3 py-2.5 text-sm font-medium rounded-lg transition-all"
                    onClick={() => setIsOpen(false)}
                  >
                    <FiList class="mr-3" size={18} />
                    List
                  </A>
                  <A
                    href={`/projects/${currentProjectId()}/dashboard`}
                    activeClass="bg-violet-50 dark:bg-violet-500/10 text-violet-600 dark:text-violet-400 font-semibold"
                    inactiveClass="text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                    class="flex items-center px-3 py-2.5 text-sm font-medium rounded-lg transition-all"
                    onClick={() => setIsOpen(false)}
                  >
                    <FiBarChart2 class="mr-3" size={18} />
                    Dashboard
                  </A>
                  <A
                    href={`/projects/${currentProjectId()}/sprints`}
                    activeClass="bg-violet-50 dark:bg-violet-500/10 text-violet-600 dark:text-violet-400 font-semibold"
                    inactiveClass="text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                    class="flex items-center px-3 py-2.5 text-sm font-medium rounded-lg transition-all"
                    onClick={() => setIsOpen(false)}
                  >
                    <FiGitBranch class="mr-3" size={18} />
                    Sprints
                  </A>
                  <A
                    href={`/projects/${currentProjectId()}/calendar`}
                    activeClass="bg-violet-50 dark:bg-violet-500/10 text-violet-600 dark:text-violet-400 font-semibold"
                    inactiveClass="text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                    class="flex items-center px-3 py-2.5 text-sm font-medium rounded-lg transition-all"
                    onClick={() => setIsOpen(false)}
                  >
                    <FiCalendar class="mr-3" size={18} />
                    Calendar
                  </A>
                  <A
                    href={`/projects/${currentProjectId()}/settings`}
                    activeClass="bg-violet-50 dark:bg-violet-500/10 text-violet-600 dark:text-violet-400 font-semibold"
                    inactiveClass="text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                    class="flex items-center px-3 py-2.5 text-sm font-medium rounded-lg transition-all"
                    onClick={() => setIsOpen(false)}
                  >
                    <FiSettings class="mr-3" size={18} />
                    Settings
                  </A>
                </div>
              </div>
            </Show>

            {/* All Projects List */}
            <Show when={!currentProjectId() || (projects() && projects()!.length > 1)}>
              <div>
                <div class="px-3 pb-2">
                  <p class="text-xs font-bold text-[var(--color-text-tertiary)] uppercase tracking-wider">
                    {currentProjectId() ? 'Other Projects' : 'Projects'}
                  </p>
                </div>
                <div class="space-y-0.5">
                  <For each={projects()}>
                    {(project) => (
                      <Show when={!currentProjectId() || project.id !== currentProjectId()}>
                        <A
                          href={`/projects/${project.id}/board`}
                          class="block px-3 py-2 text-sm font-medium text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] rounded-lg transition-all"
                          onClick={() => setIsOpen(false)}
                        >
                          {project.name}
                        </A>
                      </Show>
                    )}
                  </For>
                </div>
              </div>
            </Show>

            {/* Settings */}
            <div class="pt-4 border-t border-[var(--color-border-light)]">
              <A
                href="/settings"
                activeClass="bg-violet-50 dark:bg-violet-500/10 text-violet-600 dark:text-violet-400 font-semibold"
                inactiveClass="text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                class="flex items-center px-3 py-2.5 text-sm font-medium rounded-lg transition-all"
                onClick={() => setIsOpen(false)}
              >
                <FiSettings class="mr-3" size={18} />
                Settings
              </A>
            </div>
          </nav>

          {/* Footer */}
          <div class="px-4 py-4 border-t border-[var(--color-border-light)]">
            <p class="text-xs text-[var(--color-text-tertiary)] font-medium">
              FlexPM v1.0
            </p>
          </div>
        </div>
      </div>

      {/* Mobile overlay */}
      <Show when={isOpen()}>
        <div
          class="fixed inset-0 bg-black bg-opacity-50 z-0 lg:hidden"
          onClick={() => setIsOpen(false)}
        />
      </Show>
    </>
  );
};

export default Sidebar;
