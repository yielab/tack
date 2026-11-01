import { type Component, For, Match, Show, Switch } from 'solid-js';
import type { CompareCell, CompareModel } from './map';

const MUTED = { color: 'var(--color-text-secondary)' } as const;

/** One value: a sentence, a list, or "not reported" with the reason both on hover and for a screen reader. */
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
      {(c) => {
        const reason = () => (c() as { reason: string }).reason;
        return (
          <span data-missing="true" title={reason()} style={{ color: 'var(--color-text-tertiary)', 'font-style': 'italic' }}>
            not reported
            <span class="sr-only">{`. ${reason()}`}</span>
          </span>
        );
      }}
    </Match>
  </Switch>
);

/**
 * Compare: the teams as columns and what each is like as rows, so two can be read against each other.
 * A fact a team did not report reads "not reported"; nothing is filled in for it.
 */
const Compare: Component<{ model: CompareModel }> = (props) => (
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
          {`${props.model.teams.length} ${props.model.teams.length === 1 ? 'team' : 'teams'} side by side. "not reported" means the team did not say; hover it for why.`}
        </Show>
      </p>
    </div>
    <Show when={props.model.teams.length > 0}>
      <div class="min-w-0 overflow-x-auto" role="region" aria-label="Teams compared" tabindex="0">
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
                  <For each={row.cells}>
                    {(cell) => (
                      <td class="px-2 py-1.5 align-top" style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}>
                        <Cell cell={cell} ordered={row.key === 'steps'} />
                      </td>
                    )}
                  </For>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </Show>
  </section>
);

export default Compare;
