import { type Component, For, Match, Show, Switch, createSignal, onCleanup, onMount } from 'solid-js';
import type { CompareCell, CompareModel } from './map';

const MUTED = { color: 'var(--color-text-secondary)' } as const;

/** What a column header already says of a team no computer reports; the cells under it need not say it again. */
const TEAM_UNREPORTED = 'No computer reports this team';

/** "not reported" with the reason beside it, in small muted text; on hover and for a screen reader as well. */
const Missing: Component<{ reason: string; teams?: number }> = (props) => (
  <span data-missing="true" title={props.reason} style={{ color: 'var(--color-text-tertiary)', 'font-style': 'italic' }}>
    {props.teams && props.teams > 1 ? `not reported by any of the ${props.teams} teams` : 'not reported'}
    <Show
      when={!props.reason.startsWith(TEAM_UNREPORTED)}
      fallback={<span class="sr-only">{`. ${props.reason}`}</span>}
    >
      <span class="block text-caption font-normal not-italic" data-testid="compare-reason">{props.reason}</span>
    </Show>
  </span>
);

/** One value: a sentence, a list, or "not reported" with its reason. */
const Cell: Component<{ cell: CompareCell; ordered: boolean }> = (props) => (
  <Switch>
    <Match when={props.cell.kind === 'text' && props.cell}>{(c) => <span>{(c() as { text: string }).text}</span>}</Match>
    <Match when={props.cell.kind === 'list' && props.cell}>
      {(c) => {
        const items = () => (c() as { items: string[] }).items;
        return props.ordered ? (
          <ol class="flex min-w-0 list-decimal flex-col gap-0.5 pl-4">
            <For each={items()}>{(item) => <li>{item}</li>}</For>
          </ol>
        ) : (
          <ul class="flex min-w-0 flex-col gap-0.5">
            <For each={items()}>{(item) => <li>{item}</li>}</For>
          </ul>
        );
      }}
    </Match>
    <Match when={props.cell.kind === 'missing' && props.cell}>
      {(c) => <Missing reason={(c() as { reason: string }).reason} />}
    </Match>
  </Switch>
);

/** A row in which no team said anything: one line across the table, with the reasons once. */
const allMissing = (cells: CompareCell[]) => cells.length > 0 && cells.every((c) => c.kind === 'missing');
const reasonsOf = (cells: CompareCell[]) =>
  [...new Set(cells.map((c) => (c as { reason: string }).reason))].join(' ');

/** Whether the table is wider than its frame, and which side still has teams out of sight. */
function useScrollEdges() {
  const [left, setLeft] = createSignal(false);
  const [right, setRight] = createSignal(false);
  let frame: HTMLDivElement | undefined;
  const measure = () => {
    if (!frame) return;
    setLeft(frame.scrollLeft > 1);
    setRight(frame.scrollLeft + frame.clientWidth < frame.scrollWidth - 1);
  };
  onMount(() => {
    measure();
    if (typeof ResizeObserver === 'undefined' || !frame) return;
    const observer = new ResizeObserver(measure);
    observer.observe(frame);
    const table = frame.querySelector('table');
    if (table) observer.observe(table);
    onCleanup(() => observer.disconnect());
  });
  return { left, right, measure, ref: (el: HTMLDivElement) => { frame = el; } };
}

const EDGE = { position: 'absolute', top: 0, bottom: 0, width: '2rem', 'pointer-events': 'none' } as const;

/**
 * Compare: the teams as columns and what each is like as rows, so two can be read against each other.
 * A fact a team did not report reads "not reported"; nothing is filled in for it.
 */
const Compare: Component<{ model: CompareModel }> = (props) => {
  const edges = useScrollEdges();
  return (
  <section
    class="flex min-w-0 flex-col gap-3 rounded-card px-4 py-3"
    data-testid="compare"
    aria-labelledby="compare-title"
    style={{ 'background-color': 'var(--color-bg-panel)' }}
  >
    <div class="flex min-w-0 flex-col gap-1">
      <h3 id="compare-title" class="text-lead font-bold" style={{ color: 'var(--color-text-primary)' }}>Compare the teams</h3>
      <p class="text-small" data-testid="compare-summary" style={MUTED}>
        <Show
          when={props.model.teams.length > 0}
          fallback="No agent reports a team, and no profile names one, so there is nothing to compare."
        >
          {`${props.model.teams.length} ${props.model.teams.length === 1 ? 'team' : 'teams'} side by side. "not reported" means the team did not say, and the reason is beside it.`}
        </Show>
      </p>
    </div>
    <Show when={props.model.teams.length > 0}>
      <div class="relative min-w-0">
      <div ref={edges.ref} onScroll={edges.measure} class="min-w-0 overflow-x-auto" role="region" aria-label="Teams compared" tabindex="0">
        <table class="w-full border-collapse text-left text-small" style={{ 'min-width': 'max-content' }}>
          <thead>
            <tr>
              <th scope="col" class="px-2 py-1.5 align-bottom">
                <span class="sr-only">What is compared</span>
              </th>
              <For each={props.model.teams}>
                {(team) => (
                  <th
                    scope="col"
                    data-testid="compare-team"
                    class="px-2 py-1.5 align-bottom font-bold"
                    style={{ 'min-width': '11rem', 'max-width': '18rem', 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}
                  >
                    {team.name}
                    <Show when={!team.reported}>
                      <span class="block text-caption font-normal" style={{ color: 'var(--color-text-tertiary)' }}>No computer reports it</span>
                    </Show>
                  </th>
                )}
              </For>
            </tr>
          </thead>
          <tbody>
            <For each={props.model.rows}>
              {(row) => (
                <tr data-testid="compare-row" data-row={row.key} style={{ 'border-top': '1px solid var(--color-border-light)' }}>
                  <th scope="row" class="whitespace-nowrap px-2 py-1.5 align-top font-semibold" style={MUTED}>{row.label}</th>
                  <Show
                    when={!allMissing(row.cells)}
                    fallback={
                      <td colSpan={row.cells.length} data-collapsed="true" class="px-2 py-1.5 align-top" style={{ 'overflow-wrap': 'anywhere' }}>
                        <Missing reason={reasonsOf(row.cells)} teams={row.cells.length} />
                      </td>
                    }
                  >
                    <For each={row.cells}>
                      {(cell) => (
                        <td class="px-2 py-1.5 align-top" style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}>
                          <Cell cell={cell} ordered={row.key === 'steps'} />
                        </td>
                      )}
                    </For>
                  </Show>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
      <Show when={edges.left()}>
        <span aria-hidden="true" data-testid="compare-edge-left" class="grid place-items-center" style={{ ...EDGE, left: 0, background: 'linear-gradient(to right, var(--color-bg-panel), transparent)', color: 'var(--color-text-secondary)' }}>‹</span>
      </Show>
      <Show when={edges.right()}>
        <span aria-hidden="true" data-testid="compare-edge-right" class="grid place-items-center" style={{ ...EDGE, right: 0, background: 'linear-gradient(to left, var(--color-bg-panel), transparent)', color: 'var(--color-text-secondary)' }}>›</span>
      </Show>
      </div>
      <Show when={edges.left() || edges.right()}>
        <p class="text-caption" data-testid="compare-scroll-hint" style={MUTED}>
          {edges.right() ? 'More teams are out of sight to the right. Scroll the table, or focus it and use the arrow keys.' : 'More teams are out of sight to the left.'}
        </p>
      </Show>
    </Show>
  </section>
  );
};

export default Compare;
