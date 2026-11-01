import { type Component, For, Show, createSignal } from 'solid-js';
import { A, useNavigate } from '@solidjs/router';
import {
  NODE_H,
  NODE_W,
  clip,
  columnHeaderId,
  linkPath,
  navigate,
  routesText,
  type Direction,
  type MapFix,
  type MapModel,
  type MapNode,
} from './map';

const KEYS: Record<string, Direction> = { ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right' };

const MUTED = { color: 'var(--color-text-secondary)' } as const;

/** The fix as a short line that fits a box; the whole of it is the link's accessible name and title. */
const fixShort = (fix: MapFix): string => (fix.to === 'automation-teams' ? 'Fix: set it in Teams' : 'Fix: Agents → profiles');

const fixHref = (fix: MapFix, projectId: string): string =>
  fix.to === 'automation-teams' ? `/projects/${projectId}/settings?tab=automation` : '/agents';

/** The sentence a screen reader hears for a node: what it is, what it says of itself and what it opens. */
function nodeWords(node: MapNode): string {
  const parts = [`${node.kind === 'agent' ? 'Agent' : node.kind[0].toUpperCase() + node.kind.slice(1)} ${node.label}${node.group ? ` of ${node.group}` : ''}`];
  if (node.sub) parts.push(node.sub);
  if (node.isDefault) parts.push("the project's default profile");
  if (node.dashed && node.reason) parts.push(node.reason);
  parts.push(`${node.tasks} open ${node.tasks === 1 ? 'task' : 'tasks'}`);
  if (node.stranded > 0) parts.push(`${node.stranded} ${node.stranded === 1 ? 'task' : 'tasks'} nobody will take`);
  parts.push('Press Enter to show it in Flow');
  return parts.join('. ');
}

/**
 * The Map: four columns of boxes, Roles, Profiles, Teams and Members, with a line wherever a task can
 * go from one to the next, as wide as the open tasks routed along it. It is a view, so editing stays
 * where it is; a box or a column title opens Flow narrowed to it. Each box is a stop of the keyboard
 * (arrow keys move between them), and the same routes are written out under "Routes as text".
 */
const MapView: Component<{ model: MapModel; projectId: string; onSelect: (id: string) => void }> = (props) => {
  const go = useNavigate();
  const model = () => props.model;
  let svg: SVGSVGElement | undefined;
  const firstStop = () => columnHeaderId('roles');
  const [stop, setStop] = createSignal<string>(firstStop());
  const [ring, setRing] = createSignal<string | null>(null);
  const [pointed, setPointed] = createSignal<string | null>(null);
  const active = () => ring() ?? pointed();
  const hasStop = () => stop() === firstStop() || model().nodes.some((n) => n.id === stop()) || model().columns.some((c) => columnHeaderId(c.key) === stop());

  const focusStop = (id: string) => {
    setStop(id);
    [...(svg?.querySelectorAll<SVGGElement>('[data-node]') ?? [])].find((el) => el.getAttribute('data-node') === id)?.focus();
  };
  const keys = (id: string) => (e: KeyboardEvent) => {
    const direction = KEYS[e.key];
    if (direction) {
      e.preventDefault();
      const next = navigate(model(), id, direction);
      if (next) focusStop(next);
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      props.onSelect(id);
    }
  };
  const tabbable = (id: string) => ((hasStop() ? stop() : firstStop()) === id ? 0 : -1);
  const linked = (link: { from: string; to: string }) => {
    const a = active();
    return !!a && (link.from === a || link.to === a);
  };
  const routes = () => routesText(model());
  const counts = () => ({
    roles: model().nodes.filter((n) => n.column === 'roles').length,
    profiles: model().nodes.filter((n) => n.column === 'profiles').length,
    teams: model().nodes.filter((n) => n.column === 'teams').length,
  });

  return (
    <section
      class="flex min-w-0 flex-col gap-3 rounded-card px-4 py-3"
      data-testid="map"
      aria-labelledby="map-title"
      style={{ 'background-color': 'var(--color-bg-panel)' }}
    >
      <div class="flex min-w-0 flex-col gap-1">
        <h3 id="map-title" class="text-lead font-bold" style={{ color: 'var(--color-text-primary)' }}>Who can do what</h3>
        <p class="text-small" data-testid="map-summary" style={MUTED}>
          {`${counts().roles} ${counts().roles === 1 ? 'role' : 'roles'} lead to ${counts().profiles} ${counts().profiles === 1 ? 'profile' : 'profiles'}, which run on ${counts().teams} ${counts().teams === 1 ? 'team or agent' : 'teams and agents'}. A line is as wide as the open tasks routed along it, and a line with none appears when you point at or focus a box. Press a box or a column title to see it in Flow.`}
        </p>
        <p class="text-caption" style={{ color: 'var(--color-text-tertiary)' }}>
          A dashed box has nothing behind it: a role with no profile, or a team no profile names.
        </p>
      </div>

      <Show when={model().nodes.length > 0} fallback={<p class="text-small" style={MUTED}>No role, profile or team to draw yet.</p>}>
        <div class="min-w-0 overflow-x-auto" data-testid="map-scroll">
          <svg
            ref={svg}
            viewBox={`0 0 ${model().width} ${model().height}`}
            width={model().width}
            height={model().height}
            role="group"
            aria-labelledby="map-svg-title"
            aria-describedby="map-svg-desc"
            style={{ display: 'block', 'max-width': 'none' }}
          >
            <title id="map-svg-title">Roles, profiles, teams and members</title>
            <desc id="map-svg-desc">
              Four columns from left to right. A line joins a role to its profile, a profile to the team or agent that runs it, and a team to its
              members. The routes are also written out under Routes as text.
            </desc>

            <g aria-hidden="true" data-testid="map-links" fill="none" stroke-linecap="butt">
              <For each={model().links}>
                {(link) => (
                  <Show when={link.tasks > 0 || linked(link)}>
                  <path
                    data-testid="map-link"
                    data-link={link.id}
                    data-tasks={link.tasks}
                    d={linkPath(model(), link)}
                    stroke="var(--color-text-tertiary)"
                    stroke-width={link.width}
                    stroke-opacity={linked(link) ? 0.95 : 0.5}
                  />
                  </Show>
                )}
              </For>
            </g>

            <For each={model().columns}>
              {(col) => {
                const id = columnHeaderId(col.key);
                const words = `${col.title}. Press Enter to show every ${col.key.slice(0, -1)} in Flow`;
                return (
                  <g
                    data-testid="map-column"
                    data-node={id}
                    role="button"
                    tabindex={tabbable(id)}
                    aria-label={words}
                    style={{ cursor: 'pointer', outline: 'none' }}
                    onClick={() => props.onSelect(id)}
                    onKeyDown={keys(id)}
                    onFocus={() => {
                      setStop(id);
                      setRing(id);
                    }}
                    onBlur={() => setRing(null)}
                  >
                    <rect
                      x={col.x}
                      y={col.y}
                      width={NODE_W}
                      height={28}
                      rx="6"
                      fill="var(--color-bg-subtle)"
                      stroke={ring() === id ? 'var(--color-accent-ink)' : 'none'}
                      stroke-width="2"
                    />
                    <text x={col.x + 10} y={col.y + 19} font-size="12" font-weight="700" fill="var(--color-text-secondary)" style={{ 'text-transform': 'uppercase', 'letter-spacing': '0.08em' }}>
                      {col.title}
                    </text>
                  </g>
                );
              }}
            </For>

            <For each={model().nodes}>
              {(node) => (
                <>
                  <g
                    data-testid="map-node"
                    data-node={node.id}
                    data-kind={node.kind}
                    data-dashed={node.dashed ? 'true' : undefined}
                    data-default={node.isDefault ? 'true' : undefined}
                    role="button"
                    tabindex={tabbable(node.id)}
                    aria-label={nodeWords(node)}
                    style={{ cursor: 'pointer', outline: 'none' }}
                    onClick={() => props.onSelect(node.id)}
                    onKeyDown={keys(node.id)}
                    onFocus={() => {
                      setStop(node.id);
                      setRing(node.id);
                    }}
                    onBlur={() => setRing(null)}
                    onMouseEnter={() => setPointed(node.id)}
                    onMouseLeave={() => setPointed(null)}
                  >
                    <title>{`${node.group ? `${node.group} · ` : ''}${node.label}${node.sub ? ` · ${node.sub}` : ''}${node.dashed && node.reason ? `. ${node.reason}` : ''}`}</title>
                    <rect
                      x={node.x}
                      y={node.y}
                      width={NODE_W}
                      height={NODE_H}
                      rx="8"
                      fill={node.dashed ? 'var(--run-needs-soft)' : 'var(--color-bg-elevated)'}
                      stroke={ring() === node.id ? 'var(--color-accent-ink)' : node.dashed ? 'var(--run-needs-ink)' : node.isDefault ? 'var(--color-accent-ink)' : 'var(--color-border-medium)'}
                      stroke-width={ring() === node.id || node.isDefault ? 2 : 1}
                      stroke-dasharray={node.dashed ? '6 4' : undefined}
                    />
                    <text x={node.x + 10} y={node.y + 20} font-size="13" font-weight="700" fill="var(--color-text-primary)">
                      {clip(node.group ? `${node.group} · ${node.label}` : node.label, 21)}
                    </text>
                    <Show when={node.sub}>
                      <text x={node.x + 10} y={node.y + 37} font-size="11" fill="var(--color-text-secondary)">
                        {clip(node.sub!, 30)}
                      </text>
                    </Show>
                    <Show when={node.tasks > 0}>
                      <text x={node.x + NODE_W - 10} y={node.y + 20} font-size="11" font-weight="700" text-anchor="end" fill="var(--color-text-secondary)">
                        {node.tasks}
                      </text>
                    </Show>
                  </g>
                  <Show when={node.fix}>
                    {(fix) => (
                      <a
                        href={fixHref(fix(), props.projectId)}
                        data-testid="map-fix"
                        aria-label={`Fix: ${fix().text}`}
                        onClick={(e) => {
                          e.preventDefault();
                          go(fixHref(fix(), props.projectId));
                        }}
                      >
                        <text x={node.x + 10} y={node.y + 54} font-size="11" font-weight="600" fill="var(--color-accent-ink)" style={{ 'text-decoration': 'underline' }}>
                          <title>{`Fix: ${fix().text}`}</title>
                          {fixShort(fix())}
                        </text>
                      </a>
                    )}
                  </Show>
                </>
              )}
            </For>
          </svg>
        </div>

        <details data-testid="map-routes" class="min-w-0">
          <summary class="cursor-pointer text-small font-bold" style={{ color: 'var(--color-text-primary)' }}>Routes as text</summary>
          <div class="mt-2 flex min-w-0 flex-col gap-3">
            <For each={routes()}>
              {(group) => (
                <Show when={group.lines.length > 0}>
                  <section class="min-w-0" aria-labelledby={`map-routes-${group.key}`}>
                    <h4 id={`map-routes-${group.key}`} class="text-caption font-bold uppercase tracking-widest" style={{ color: 'var(--color-text-tertiary)' }}>{group.title}</h4>
                    <ul class="mt-1 flex min-w-0 flex-col gap-1" data-testid={`map-routes-${group.key}`}>
                      <For each={group.lines}>
                        {(line) => (
                          <li data-route={line.id} class="min-w-0 text-small" style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}>
                            {line.text}
                            <Show when={line.fix}>
                              {(fix) => (
                                <>
                                  {' Fix: '}
                                  <A href={fixHref(fix(), props.projectId)} class="font-semibold underline" style={{ color: 'var(--color-accent-ink)' }}>{fix().text}</A>
                                </>
                              )}
                            </Show>
                          </li>
                        )}
                      </For>
                    </ul>
                  </section>
                </Show>
              )}
            </For>
          </div>
        </details>
      </Show>
    </section>
  );
};

export default MapView;
