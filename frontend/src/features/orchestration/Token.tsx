import { type Component, For, Show } from 'solid-js';
import { RunState, TypeTile } from '../../shared/ui';
import ItemPath from '../../shared/ui/ItemPath';
import { ageText, moveTarget, sourceText, type FlowLane, type FlowToken } from './flow';
import { useFlowUi, type MoveSource } from './context';
import MoveMenu, { type MoveChoice } from './MoveMenu';

const MUTED = { color: 'var(--color-text-secondary)' } as const;

/** The short curved arrow of a task sent back to an earlier step. */
const Arc: Component = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="flex-none">
    <path d="M13 8a5 5 0 1 1-1.8-3.8M13 2.5v3h-3" />
  </svg>
);

/** What the lanes offer a task as places to go: each other lane, off with its reason when it cannot take it. */
export function moveChoices(source: Pick<MoveSource, 'lane' | 'inFlight' | 'station'>, lanes: readonly FlowLane[]): MoveChoice[] {
  return lanes
    .filter((l) => l.key !== source.lane)
    .map((l) => {
      const target = moveTarget(source, l);
      return { key: l.key, label: l.name, reason: target.ok ? undefined : target.reason };
    });
}

/**
 * One task as a token: its type, its title (which opens the task on its Execution tab), the path of
 * what it sits under, the run's state, how long the run has been going, and what links it to other work:
 * sent back, waiting on another task, proposals it made or came from. A task that has not started can
 * be dragged to another lane's Up next, or sent with the Move to… menu.
 */
const Token: Component<{ token: FlowToken; source: MoveSource }> = (props) => {
  const ui = useFlowUi();
  const t = () => props.token;
  const age = () => (t().run && t().station !== 'done' ? ageText(t().summary?.waiting_since ?? t().summary?.created_at, ui.now()) : null);
  const movable = () => t().station === 'up-next' && !t().inFlight;
  const choices = () => moveChoices(props.source, ui.lanes());
  // A queued task is held by its run; the button stays, off, and says why.
  const offReason = () => (t().inFlight ? 'A run holds this task, so it cannot change hands' : choices().every((c) => c.reason) ? 'No other lane can take it' : null);
  const lit = () => ui.highlight() === t().id;

  return (
    <li
      data-testid="flow-token"
      data-item-id={t().id}
      data-station={t().station}
      data-state={t().run?.state ?? 'none'}
      draggable={movable()}
      onDragStart={(e) => {
        if (!movable()) return;
        ui.setDragged(props.source);
        e.dataTransfer?.setData('text/plain', t().id);
        if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
      }}
      onDragEnd={() => ui.setDragged(null)}
      class={`flex min-w-0 flex-col gap-1 rounded-item px-3 py-2 ${ui.arrived().has(t().id) ? 'flow-arrive' : ''}`}
      style={{
        'background-color': 'var(--color-bg-elevated)',
        'box-shadow': lit()
          ? '0 0 0 2px var(--color-accent-ink)'
          : t().run
            ? `inset 0 0 0 1px var(--run-${t().run!.state}-line)`
            : 'inset 0 0 0 1px var(--color-border-light)',
        cursor: movable() ? 'grab' : undefined,
      }}
    >
      <div class="flex min-w-0 items-start gap-2">
        <TypeTile type={t().item.item_type} class="flex-none" />
        <button
          type="button"
          data-testid="flow-token-open"
          class="min-w-0 flex-1 text-left text-small font-semibold focus:outline-none focus-visible:ring-2"
          style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)', background: 'none', border: 0, padding: 0, cursor: 'pointer' }}
          title="Open the Execution tab"
          onClick={() => ui.open(t().id, t().summary)}
        >
          {t().item.title}
        </button>
        <Show when={t().run}>{(run) => <RunState state={run().state} docket={run().docket} size="dot" />}</Show>
        <Show when={t().station === 'up-next'}>
          <MoveMenu
            title={t().item.title}
            choices={choices()}
            offReason={offReason()}
            onPick={(key) => {
              const lane = ui.lanes().find((l) => l.key === key);
              if (lane) ui.requestMove(props.source, lane);
            }}
          />
        </Show>
      </div>
      <ItemPath item={t().item} byId={ui.byId()} projectId={ui.projectId()} class="text-caption" />
      <Show when={t().planning}>
        <p class="text-caption font-semibold" data-testid="flow-token-planning" style={{ color: 'var(--color-accent-ink)' }}>
          Planning
          <span class="sr-only"> — this run proposes the items of a group; it does not do them</span>
        </p>
      </Show>
      <div class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-0.5 text-caption" style={MUTED}>
        <Show when={age()}>
          {(text) => (
            <span title={`The run started ${text()} ago`} data-testid="flow-token-age">
              {text()}
            </span>
          )}
        </Show>
        <Show when={t().sentBack}>
          {(back) => (
            <span
              class="inline-flex items-center gap-1"
              data-testid="flow-token-sent-back"
              title={back().from ? `Sent back by ${back().from}` : 'Sent back'}
              style={{ color: 'var(--run-needs-ink)' }}
            >
              <Arc />
              {back().max == null ? back().n : `${back().n}/${back().max}`}
              <span class="sr-only">{back().from ? ` — sent back by ${back().from}` : ' — sent back'}</span>
            </span>
          )}
        </Show>
        <Show when={t().proposed > 0}>
          <span data-testid="flow-token-proposed" title="Items created from this task's proposals">
            {`→ ${t().proposed} proposed`}
          </span>
        </Show>
        <Show when={t().fromTitle}>
          {(from) => (
            <span data-testid="flow-token-from" style={{ 'overflow-wrap': 'anywhere' }}>{`from: ${from()}`}</span>
          )}
        </Show>
      </div>
      <For each={t().blockers}>
        {(blocker) => {
          const laneName = () => ui.lanes().find((l) => l.key === blocker.lane)?.name;
          return (
            <button
              type="button"
              data-testid="flow-token-after"
              class="min-w-0 text-left text-caption focus:outline-none focus-visible:ring-2"
              style={{ ...MUTED, 'overflow-wrap': 'anywhere', background: 'none', border: 0, padding: 0, cursor: 'help' }}
              title="Point at the task it waits for"
              onMouseEnter={() => ui.setHighlight(blocker.id)}
              onMouseLeave={() => ui.setHighlight(null)}
              onFocus={() => ui.setHighlight(blocker.id)}
              onBlur={() => ui.setHighlight(null)}
            >
              {`after: ${blocker.title}${laneName() ? ` · ${laneName()}` : ''}`}
            </button>
          );
        }}
      </For>
      <Show when={t().upNext}>
        {(next) => (
          <p class="text-caption" data-testid="flow-token-start" style={{ ...MUTED, 'overflow-wrap': 'anywhere' }}>
            {next().text}
            <Show when={next().queuedReason}>
              {(why) => <span data-testid="flow-token-queued-reason">{` · ${why()}`}</span>}
            </Show>
          </p>
        )}
      </Show>
      <Show when={t().route?.kind === 'lane' && t().station === 'up-next' && t().route}>
        {(route) => (
          <p class="sr-only">{`Goes to ${(route() as { profile: { name: string } }).profile.name}, ${sourceText(route() as never)}`}</p>
        )}
      </Show>
    </li>
  );
};

export default Token;
