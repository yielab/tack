import { type Component, For, Show } from 'solid-js';
import { A } from '@solidjs/router';
import { Button } from '../../shared/ui';
import ProfilePicker from '../../shared/runWithAgent/ProfilePicker';
import { showsNobody, visibleLanes, type Flow, type FlowFilter, type NobodyEntry, type UnusedTeam } from './flow';
import { useFlowUi } from './context';
import FlowLaneView from './Lane';
import MoveMenu from './MoveMenu';
import { moveChoices } from './Token';

const MUTED = { color: 'var(--color-text-secondary)' } as const;

/** One task nobody will take: why, and the choice of who does it right there. */
const NobodyRow: Component<{ entry: NobodyEntry }> = (props) => {
  const ui = useFlowUi();
  const e = () => props.entry;
  const source = () => ({ id: e().id, title: e().item.title, lane: null, inFlight: false, station: 'up-next' as const });
  const choices = () => moveChoices(source(), ui.lanes());
  return (
    <li
      data-testid="flow-nobody-entry"
      data-item-id={e().id}
      draggable={true}
      onDragStart={(ev) => {
        ui.setDragged(source());
        ev.dataTransfer?.setData('text/plain', e().id);
        if (ev.dataTransfer) ev.dataTransfer.effectAllowed = 'move';
      }}
      onDragEnd={() => ui.setDragged(null)}
      class="flex min-w-0 flex-col gap-1.5 rounded-item px-3 py-2"
      style={{ 'background-color': 'var(--color-bg-elevated)', 'box-shadow': 'inset 0 0 0 1px var(--run-needs-line)', cursor: 'grab' }}
    >
      <div class="flex min-w-0 items-start gap-2">
        <button
          type="button"
          class="min-w-0 flex-1 text-left text-small font-semibold focus:outline-none focus-visible:ring-2"
          style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)', background: 'none', border: 0, padding: 0, cursor: 'pointer' }}
          title="Open the task"
          onClick={() => ui.open(e().id)}
        >
          {e().item.title}
        </button>
        <MoveMenu
          title={e().item.title}
          choices={choices()}
          offReason={choices().every((c) => c.reason) ? 'No lane can take it' : null}
          onPick={(key) => {
            const lane = ui.lanes().find((l) => l.key === key);
            if (lane) ui.requestMove(source(), lane);
          }}
        />
      </div>
      <Show when={e().parentTitle}>
        {(parent) => <p class="text-caption" style={{ ...MUTED, 'overflow-wrap': 'anywhere' }}>{parent()}</p>}
      </Show>
      <p class="text-caption" data-testid="flow-nobody-reason" data-reason={e().route.reason} style={{ color: 'var(--run-needs-ink)', 'overflow-wrap': 'anywhere' }}>
        {e().text}
      </p>
      <details data-testid="flow-nobody-picker">
        <summary class="cursor-pointer text-small font-semibold" style={{ color: 'var(--color-accent-ink)' }}>Who does it</summary>
        <div class="mt-2 min-w-0">
          <ProfilePicker
            label={`Who does ${e().item.title}`}
            value={null}
            profiles={ui.profiles()}
            teams={ui.teams()}
            reports={ui.reports()}
            onChange={(profile) => ui.chooseProfile(e().id, profile)}
          />
        </div>
      </details>
    </li>
  );
};

/** The lane at the top, in the warning tone, for tasks no lane will take. Only drawn when it has some. */
const NobodyLane: Component<{ entries: NobodyEntry[] }> = (props) => (
  <section
    data-testid="flow-nobody"
    aria-labelledby="flow-nobody-title"
    class="flex min-w-0 flex-col gap-2 rounded-card px-4 py-3"
    style={{ 'background-color': 'var(--run-needs-soft)', 'box-shadow': 'inset 0 0 0 1px var(--run-needs-line)' }}
  >
    <h3 id="flow-nobody-title" class="text-lead font-bold" style={{ color: 'var(--run-needs-ink)' }}>
      Nobody will take it
      <span class="tabular-nums" style={{ 'margin-left': '0.5rem' }}>{props.entries.length}</span>
    </h3>
    <p class="text-caption" style={{ color: 'var(--run-needs-ink)' }}>
      These tasks go to no lane, so nothing will start them. Choose who does each one, or drag it to a lane's Up next.
    </p>
    <ul aria-label="Tasks nobody will take" class="grid min-w-0 gap-2" style={{ 'grid-template-columns': 'repeat(auto-fill, minmax(min(100%, 16rem), 1fr))' }}>
      <For each={props.entries}>{(entry) => <NobodyRow entry={entry} />}</For>
    </ul>
  </section>
);

/** Teams no profile of this project names: listed, each with "Make a profile for it" off and the reason. */
const UnusedRow: Component<{ teams: UnusedTeam[] }> = (props) => (
  <details data-testid="flow-unused" class="min-w-0 rounded-card px-4 py-3" style={{ 'background-color': 'var(--color-bg-panel)' }}>
    <summary class="cursor-pointer text-small font-bold" style={{ color: 'var(--color-text-primary)' }}>
      {`Teams not used here (${props.teams.length})`}
    </summary>
    <ul aria-label="Teams not used here" class="mt-2 flex min-w-0 flex-col gap-2">
      <For each={props.teams}>
        {(team) => (
          <li data-testid={`flow-unused-${team.name}`} class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
            <span class="min-w-0 text-small font-semibold" style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}>{team.name}</span>
            <Button type="button" size="sm" variant="secondary" disabled reason={team.reason}>
              Make a profile for it
            </Button>
            <A href="/agents" class="text-caption font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }}>Agents</A>
          </li>
        )}
      </For>
    </ul>
  </details>
);

/**
 * Flow: the Nobody lane when it has tasks, one lane per team or single agent (those that need you
 * first, then those running, then the idle ones folded to their header), and the teams this project
 * does not use. The filters of the summary row narrow it.
 */
const FlowView: Component<{ flow: Flow; filters: ReadonlySet<FlowFilter>; dailyLimit: number | null }> = (props) => {
  const lanes = () => visibleLanes(props.flow, props.filters);
  const empty = () => props.flow.lanes.length === 0 && props.flow.nobody.length === 0;
  return (
    <div class="flex min-w-0 flex-col gap-3" data-testid="flow">
      <Show when={props.flow.nobody.length > 0 && showsNobody(props.filters)}>
        <NobodyLane entries={props.flow.nobody} />
      </Show>
      <Show when={!empty()} fallback={<p class="text-small" style={MUTED}>No lane has anything to show yet: no team has a profile in this project, and no task is ready to start.</p>}>
        <ol aria-label="Lanes" class="flex min-w-0 flex-col gap-3" data-testid="flow-lanes">
          <For each={lanes()}>{(lane) => <FlowLaneView lane={lane} limit={props.dailyLimit} />}</For>
        </ol>
        <Show when={props.filters.size > 0 && lanes().length === 0 && !props.filters.has('nobody')}>
          <p class="text-small" style={MUTED}>No lane has a task in that state.</p>
        </Show>
      </Show>
      <Show when={props.flow.unused.length > 0}>
        <UnusedRow teams={props.flow.unused} />
      </Show>
    </div>
  );
};

export default FlowView;
