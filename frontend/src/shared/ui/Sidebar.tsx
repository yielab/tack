import { A, useParams, useLocation, useNavigate } from '@solidjs/router';
import { createSignal, Show, For, createResource, type Component, type JSX } from 'solid-js';
import { api } from '../api';
import { getLastLens } from '../state/lastView';
import { openPalette } from '../state/commandPalette';
import { currentPalette, setPalette, PALETTES, type Palette } from '../state/palette';
import { isDarkActive, toggleTheme } from '../state/theme';
import {
  BrandMark,
  IconSearch, IconBoard, IconList, IconTable, IconCalendar, IconTimeline, IconSprint,
  IconOverview, IconProjects, IconTemplates, IconAgent, IconSettings, IconSun, IconMoon,
  IconChevronDown, type IconProps,
} from './icons';
import KbdHint from './KbdHint';
import { useVocab } from '../vocab/useVocab';

type Glyph = Component<IconProps>;

/** A nav row: icon + label, active = accent-soft pill. */
const NavButton: Component<{
  href: string;
  icon: Glyph;
  label: string;
  end?: boolean;
  badge?: JSX.Element;
  onClick?: () => void;
}> = (p) => {
  const location = useLocation();
  const active = () => (p.end ? location.pathname === p.href : location.pathname.startsWith(p.href));
  return (
    <A
      href={p.href}
      onClick={p.onClick}
      style={{
        width: '100%',
        display: 'flex',
        'align-items': 'center',
        gap: '10px',
        padding: '7px 12px',
        'border-radius': 'var(--radius-pill)',
        'font-size': '14px',
        'font-weight': active() ? 600 : 500,
        'margin-bottom': '2px',
        background: active() ? 'var(--color-accent-soft)' : 'transparent',
        color: active() ? 'var(--color-accent-ink)' : 'var(--color-text-primary)',
      }}
      onMouseEnter={(e) => { if (!active()) e.currentTarget.style.background = 'var(--color-bg-app)'; }}
      onMouseLeave={(e) => { if (!active()) e.currentTarget.style.background = 'transparent'; }}
    >
      <p.icon size={16} />
      <span style={{ flex: 1, 'text-align': 'left' }}>{p.label}</span>
      {p.badge}
    </A>
  );
};

const SectionLabel: Component<{ label: string }> = (p) => (
  <div style={{ padding: '6px 10px 4px' }}>
    <span style={{
      'font-size': '10px', 'font-weight': 700, 'letter-spacing': '.12em',
      'text-transform': 'uppercase', color: 'var(--color-text-tertiary)',
    }}>{p.label}</span>
  </div>
);

const PaletteSwatch: Component<{ value: Palette; color: string; title: string }> = (p) => {
  const selected = () => currentPalette() === p.value;
  return (
    <button
      title={p.title}
      onClick={() => setPalette(p.value)}
      style={{
        width: '18px', height: '18px', 'border-radius': '99px', cursor: 'pointer',
        background: p.color, padding: 0,
        border: 'none',
        'box-shadow': selected()
          ? '0 0 0 2px var(--color-bg-sidebar), 0 0 0 4px var(--color-text-primary)'
          : 'none',
      }}
    />
  );
};

const Sidebar: Component = () => {
  const params = useParams();
  const navigate = useNavigate();
  const { t } = useVocab();
  const [isOpen, setIsOpen] = createSignal(false);
  const close = () => setIsOpen(false);

  const currentProjectId = () => params.id as string | undefined;
  const [projects] = createResource(() => api.projects.list());
  const [health] = createResource(() => api.system.health());

  const handleProjectSwitch = (id: string) => {
    navigate(`/projects/${id}/${getLastLens()}`);
    close();
  };

  const inner = (
    <div style={{ display: 'flex', 'flex-direction': 'column', height: '100%' }}>
      {/* brand */}
      <div style={{ padding: '20px 20px 14px', display: 'flex', 'align-items': 'center', gap: '10px' }}>
        <BrandMark size={30} />
        <div style={{ display: 'flex', 'flex-direction': 'column', 'line-height': '1' }}>
          <span style={{ 'font-family': 'var(--font-heading)', 'font-size': '22px', color: 'var(--color-text-primary)' }}>Tack</span>
          <span style={{ 'font-size': '9.5px', 'font-weight': 700, 'letter-spacing': '.14em', 'text-transform': 'uppercase', color: 'var(--color-text-tertiary)', 'margin-top': '3px' }}>self-hosted</span>
        </div>
      </div>

      {/* workspace pill */}
      <div style={{ padding: '0 14px 10px' }}>
        <div style={{
          width: '100%', display: 'flex', 'align-items': 'center', gap: '8px',
          padding: '8px 14px', 'border-radius': 'var(--radius-pill)',
          background: 'var(--color-bg-app)',
        }}>
          <span style={{ width: '8px', height: '8px', 'border-radius': '99px', background: health() ? 'var(--color-success-600)' : 'var(--color-text-tertiary)', 'flex-shrink': 0 }} />
          <span style={{ flex: 1, 'text-align': 'left', 'font-size': '13px', 'font-weight': 600, color: 'var(--color-text-primary)' }}>Local workspace</span>
          <span style={{ 'font-family': 'var(--font-mono)', 'font-size': '10.5px', color: 'var(--color-text-tertiary)' }}>tack.db</span>
        </div>
      </div>

      {/* search trigger */}
      <div style={{ padding: '0 14px 14px' }}>
        <button
          onClick={() => { openPalette(); close(); }}
          style={{
            width: '100%', display: 'flex', 'align-items': 'center', gap: '8px',
            padding: '8px 14px', 'border-radius': 'var(--radius-pill)', cursor: 'pointer',
            border: '1px solid var(--color-border-strong)', background: 'transparent',
            'font-family': 'inherit', color: 'var(--color-text-secondary)',
          }}
        >
          <IconSearch size={15} />
          <span style={{ flex: 1, 'text-align': 'left', 'font-size': '13px' }}>Search…</span>
          <KbdHint>⌃/</KbdHint>
        </button>
      </div>

      {/* nav */}
      <div style={{ flex: 1, 'overflow-y': 'auto', padding: '0 14px' }}>
        <Show when={currentProjectId()}>
          <SectionLabel label="Project" />

          {/* project switcher pill */}
          <div style={{ padding: '0 0 6px' }}>
            <div style={{ position: 'relative' }}>
              <select
                aria-label="Switch project"
                value={currentProjectId()}
                onChange={(e) => handleProjectSwitch(e.currentTarget.value)}
                style={{
                  width: '100%', appearance: 'none', cursor: 'pointer',
                  padding: '8px 32px 8px 14px', 'border-radius': 'var(--radius-pill)',
                  border: 'none', background: 'var(--color-bg-app)',
                  'font-family': 'inherit', 'font-size': '14px', 'font-weight': 600,
                  color: 'var(--color-text-primary)',
                }}
              >
                <For each={projects()}>{(p) => <option value={p.id}>{p.name}</option>}</For>
              </select>
              <span style={{ position: 'absolute', right: '12px', top: '50%', transform: 'translateY(-50%)', 'pointer-events': 'none', color: 'var(--color-text-tertiary)', display: 'flex' }}>
                <IconChevronDown size={13} />
              </span>
            </div>
          </div>

          <NavButton href={`/projects/${currentProjectId()}/board`} icon={IconBoard} label="Board" onClick={close} />
          <NavButton href={`/projects/${currentProjectId()}/list`} icon={IconList} label="List" onClick={close} />
          <NavButton href={`/projects/${currentProjectId()}/table`} icon={IconTable} label="Table" onClick={close} />
          <NavButton href={`/projects/${currentProjectId()}/calendar`} icon={IconCalendar} label="Calendar" onClick={close} />
          <NavButton href={`/projects/${currentProjectId()}/timeline`} icon={IconTimeline} label="Timeline" onClick={close} />
          <NavButton href={`/projects/${currentProjectId()}/sprint`} icon={IconSprint} label={t('sprint')} onClick={close} />
          <NavButton href={`/projects/${currentProjectId()}/overview`} icon={IconOverview} label="Overview" onClick={close} />

          <div style={{ height: '1px', background: 'var(--color-border-light)', margin: '10px 10px' }} />
        </Show>

        <SectionLabel label="Workspace" />
        <NavButton href="/projects" end icon={IconProjects} label="All projects" onClick={close} />
        <NavButton href="/templates" icon={IconTemplates} label="Templates" onClick={close} />
        <NavButton href="/agents" icon={IconAgent} label="Agents" onClick={close} />
        <NavButton href={currentProjectId() ? `/projects/${currentProjectId()}/settings` : '/settings'} icon={IconSettings} label="Settings" onClick={close} />
      </div>

      {/* footer: theme + palette + identity */}
      <div style={{ padding: '10px 18px 16px', display: 'flex', 'flex-direction': 'column', gap: '14px' }}>
        <div style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
          <button
            onClick={toggleTheme}
            title="Toggle theme"
            style={{
              width: '30px', height: '30px', 'border-radius': '99px', cursor: 'pointer',
              border: 'none', background: 'var(--color-bg-app)',
              display: 'flex', 'align-items': 'center', 'justify-content': 'center',
              color: 'var(--color-text-secondary)',
            }}
          >
            <Show when={isDarkActive()} fallback={<IconSun size={15} />}><IconMoon size={15} /></Show>
          </button>
          <div style={{ display: 'flex', 'align-items': 'center', gap: '8px', flex: 1, 'justify-content': 'flex-end' }}>
            <span style={{ 'font-size': '11px', color: 'var(--color-text-tertiary)', 'margin-right': '2px' }}>Palette</span>
            <PaletteSwatch value={PALETTES[0]} color="#2e5f7f" title="Harbor" />
            <PaletteSwatch value={PALETTES[1]} color="#0d9488" title="Teal" />
            <PaletteSwatch value={PALETTES[2]} color="#c2410c" title="Clay" />
            <PaletteSwatch value={PALETTES[3]} color="#3f6a0c" title="Graphite" />
          </div>
        </div>
        <div style={{ display: 'flex', 'align-items': 'center', gap: '9px', padding: '2px' }}>
          <span style={{ width: '28px', height: '28px', 'border-radius': '99px', background: 'var(--color-primary-600)', color: 'var(--color-on-accent)', display: 'flex', 'align-items': 'center', 'justify-content': 'center', 'font-family': 'var(--font-heading)', 'font-size': '14px', 'flex-shrink': 0 }}>T</span>
          <div style={{ flex: 1, 'line-height': '1.3', 'min-width': 0 }}>
            <div style={{ 'font-size': '13px', 'font-weight': 600, color: 'var(--color-text-primary)', 'white-space': 'nowrap', overflow: 'hidden', 'text-overflow': 'ellipsis' }}>Local</div>
            <div style={{ 'font-family': 'var(--font-mono)', 'font-size': '10px', color: 'var(--color-text-tertiary)' }}>{health() ? `v${health()!.version} · single token` : 'single token'}</div>
          </div>
        </div>
      </div>
    </div>
  );

  return (
    <>
      {/* Mobile top bar */}
      <div
        class="lg:hidden fixed top-0 left-0 right-0 z-20 border-b flex items-center justify-between px-4 py-3"
        style={{ background: 'var(--color-bg-elevated)', 'border-color': 'var(--color-border-light)', 'box-shadow': 'var(--shadow-sm)' }}
      >
        <div style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
          <BrandMark size={24} />
          <span style={{ 'font-family': 'var(--font-heading)', 'font-size': '20px', color: 'var(--color-text-primary)' }}>Tack</span>
        </div>
        <button onClick={() => setIsOpen(!isOpen())} class="p-2 rounded-lg" style={{ color: 'var(--color-text-secondary)' }} aria-label="Toggle menu">
          <Show when={isOpen()} fallback={<IconList size={22} />}><IconChevronDown size={22} /></Show>
        </button>
      </div>

      {/* Sidebar panel */}
      <div
        class={`fixed inset-y-0 left-0 z-10 transform transition-transform duration-200 ease-in-out lg:translate-x-0 lg:static ${isOpen() ? 'translate-x-0' : '-translate-x-full'}`}
        style={{ width: '252px', 'flex-shrink': 0, background: 'var(--color-bg-sidebar)' }}
      >
        {inner}
      </div>

      {/* Mobile overlay */}
      <Show when={isOpen()}>
        <div class="fixed inset-0 z-0 lg:hidden" style={{ background: 'var(--color-bg-overlay)' }} onClick={close} />
      </Show>
    </>
  );
};

export default Sidebar;
