import { type Component, type JSX, For, Show, createEffect, createMemo, createSignal } from 'solid-js';
import { A } from '@solidjs/router';
import { Avatar, Badge } from '../../shared/ui';
import { memberReachText, networkText, queueText, type TeamMember } from '../../shared/execution';
import { checkText } from '../../shared/runWithAgent/teamSteps';
import { runTally, tallySentence } from '../../shared/runWithAgent/cardRun';
import UsageMeter from '../../shared/runWithAgent/UsageMeter';
import { spaced } from '../../shared/runWithAgent/runLimit';
import { IconBranch, IconFolder, IconWho } from '../../shared/runWithAgent/runDialogIcons';
import { laneQueue, moveTarget, type FlowLane, type FlowStation, type FlowToken } from './flow';
import { useFlowUi } from './context';
import Token from './Token';

const MUTED = { color: 'var(--color-text-secondary)' } as const;
const FAINT = { color: 'var(--color-text-tertiary)' } as const;

// Glyphs are built fresh on every call: a DOM node can sit in one place only.
const glyph = (body: JSX.Element, size = 14): JSX.Element => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="flex-none">
    {body}
  </svg>
);
const CHECK_GLYPH = {
  command: () => glyph(<path d="M3 4l4 4-4 4M8.5 12.5h4.5" />),
  reviewer: () =>
    glyph(
      <>
        <path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z" />
        <circle cx="8" cy="8" r="2" />
      </>,
    ),
  person: () => <IconWho />,
  external: () => glyph(<path d="M6 3H3v10h10v-3M9 3h4v4M13 3L7.5 8.5" />),
} as const;

/** An icon with the words a screen reader and a tooltip need. */
const Mark: Component<{ words: string; children: JSX.Element; attr?: string }> = (props) => (
  <span role="img" aria-label={props.words} title={props.words} data-mark={props.attr} class="inline-flex flex-none items-center gap-1" style={MUTED}>
    {props.children}
  </span>
);

/** A member of the team as a round mark of its role: a ring when it changes the project, a dot when it can propose items. */
const MemberMark: Component<{ member: TeamMember }> = (props) => {
  const words = () => [props.member.role, props.member.model, ...memberReachText(props.member)].filter(Boolean).join(' · ');
  return (
    <li
      data-testid={`flow-member-${props.member.role}`}
      data-writes={props.member.writes_project === true ? 'true' : undefined}
      data-proposes={props.member.can_propose === true ? 'true' : undefined}
      class="relative flex-none"
    >
      <span
        class="inline-flex rounded-full"
        style={{ 'box-shadow': props.member.writes_project === true ? '0 0 0 2px var(--color-accent-ink)' : 'none', 'margin': '2px' }}
      >
        <Avatar name={props.member.role} size="sm" title={words()} />
      </span>
      <Show when={props.member.can_propose === true}>
        <span
          aria-hidden="true"
          class="absolute rounded-full"
          style={{ width: '8px', height: '8px', right: '0', top: '0', 'background-color': 'var(--color-accent2)', 'box-shadow': '0 0 0 2px var(--color-bg-panel)' }}
        />
      </Show>
      <span class="sr-only">{words()}</span>
    </li>
  );
};

/** What the team's header says of how it works; a fact the agent left out is named once, not guessed. */
const Facts: Component<{ lane: FlowLane }> = (props) => {
  const report = () => props.lane.report;
  const missing = () => {
    const r = report();
    if (!props.lane.entry || !r) return [];
    const out: string[] = [];
    if (!r.works_in) out.push('where it works');
    if (!r.decides) out.push('whether it stops for a person');
    if (!r.network) out.push('its network');
    if (!r.members) out.push('its members');
    if (!r.queue) out.push('its queue');
    return out;
  };
  return (
    <div class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-caption" style={MUTED}>
      <Show when={report()?.shape}>{(shape) => <Badge tone="neutral">{shape()}</Badge>}</Show>
      <Show when={props.lane.kind === 'agent'}>
        <Badge tone="neutral">One agent</Badge>
      </Show>
      <Show when={report()?.works_in}>
        {(where) => (
          <Mark words={where() === 'repository' ? 'Works in this repository, on a branch' : 'Works in a folder'} attr="works-in">
            {where() === 'repository' ? <IconBranch /> : <IconFolder />}
            {where() === 'repository' ? 'Repository' : 'Folder'}
          </Mark>
        )}
      </Show>
      <Show when={report()?.decides}>
        {(decides) => (
          <Mark words={decides() === 'asks_a_person' ? 'Stops to ask a person' : 'Runs to the end by itself'} attr="decides">
            {decides() === 'asks_a_person' ? <IconWho /> : glyph(<path d="M3 8h9M9 4.5L12.5 8 9 11.5" />)}
            {decides() === 'asks_a_person' ? 'Stops for you' : 'Runs to the end'}
          </Mark>
        )}
      </Show>
      <Show when={report()?.network}>
        {(network) => (
          <Mark words={networkText(network())} attr="network">
            {glyph(
              <>
                <circle cx="8" cy="8" r="6" />
                <path d="M2 8h12M8 2c2.2 2.2 2.2 9.8 0 12M8 2c-2.2 2.2-2.2 9.8 0 12" />
              </>,
            )}
            {networkText(network())}
          </Mark>
        )}
      </Show>
      <Show when={props.lane.merge}>
        {(rung) => (
          <span data-testid="flow-merge-rung" data-attended={rung().attended ? 'true' : 'false'} title="How a change from this lane gets merged">
            {rung().text}
          </span>
        )}
      </Show>
      <Show when={missing().length > 0}>
        <span data-testid="flow-not-reported" aria-disabled="true" style={FAINT}>{`Not reported: ${missing().join(', ')}`}</span>
      </Show>
    </div>
  );
};

/** Running and waiting as one bar, with the sentence beside it. */
const QueueMeter: Component<{ lane: FlowLane }> = (props) => {
  const q = () => laneQueue(props.lane);
  const total = () => Math.max(1, q().running + q().waiting);
  const words = () => `${queueText(q())}${q().reported ? '' : ' (counted on this board)'}`;
  return (
    <div class="flex min-w-0 items-center gap-2" data-testid="flow-queue">
      <div
        role="img"
        aria-label={words()}
        title={words()}
        class="flex h-2 w-24 flex-none overflow-hidden rounded-pill"
        style={{ 'background-color': 'var(--color-bg-subtle)', 'box-shadow': 'inset 0 0 0 1px var(--color-border-light)' }}
      >
        <span style={{ flex: `${q().running} 1 0`, 'background-color': 'var(--run-running-ink)' }} />
        <span style={{ flex: `${q().waiting} 1 0`, 'background-color': 'var(--run-queued-ink)', opacity: 0.45 }} />
        <span style={{ flex: `${q().running + q().waiting === 0 ? total() : 0} 1 0` }} />
      </div>
      <span aria-hidden="true" class="text-caption tabular-nums" style={MUTED}>{`${q().running} running · ${q().waiting} waiting`}</span>
    </div>
  );
};

/** Tokens the lane's runs counted, against the project's daily limit when it has one. */
const TokenMeter: Component<{ lane: FlowLane; limit: number | null }> = (props) => (
  <div class="flex min-w-0 flex-col gap-0.5" data-testid="flow-tokens-meter">
    <Show
      when={props.limit}
      fallback={
        <span class="text-caption tabular-nums" style={MUTED} title="Counted by runs open now or ended in the last 24 hours">
          {`${spaced(props.lane.tokensToday)} tokens today · no daily limit`}
        </span>
      }
    >
      {(limit) => (
        <>
          <UsageMeter used={props.lane.tokensToday} limit={limit()} testId="flow-tokens" />
          <span class="text-caption" style={FAINT} title="Counted by runs open now or ended in the last 24 hours">
            Tokens today, against the project's daily limit
          </span>
        </>
      )}
    </Show>
  </div>
);

/** One station: its name, what a step there is, and the tokens standing at it. */
const Station: Component<{ lane: FlowLane; station: FlowStation }> = (props) => {
  const ui = useFlowUi();
  const st = () => props.station;
  const [unfolded, setUnfolded] = createSignal(false);
  const isDone = () => st().key === 'done';
  const sentence = () => (st().step ? checkText(st().step!) : null);
  const label = () => `${st().label}, ${props.lane.name}`;
  const isUpNext = () => st().key === 'up-next';

  // Only a task that has not started can be dropped into Up next, and only on a lane that can take it.
  const dropped = () => {
    const source = ui.dragged();
    return source && isUpNext() ? moveTarget(source, props.lane) : null;
  };
  const accepts = () => dropped()?.ok === true;

  const sourceOf = (t: FlowToken) => ({ id: t.id, title: t.item.title, lane: t.lane, inFlight: t.inFlight, station: t.station });

  return (
    <li
      data-testid="flow-station"
      data-station={st().key}
      data-accepts={isUpNext() ? (dropped() ? String(accepts()) : undefined) : undefined}
      data-parallel={st().step?.kind === 'parallel' ? 'true' : undefined}
      class="flex min-w-0 flex-col gap-2 rounded-item px-2 py-2"
      style={{
        // Steps that run side by side are drawn as a stack of two.
        ...(st().step?.kind === 'parallel' ? { 'box-shadow': '0.3rem 0.3rem 0 -1px var(--color-border-medium)', border: '1px solid var(--color-border-medium)' } : {}),
        flex: isDone() ? '1 1 10rem' : '1 1 12rem',
        'min-width': isDone() ? '10rem' : '12rem',
        outline: dropped() ? (accepts() ? '2px dashed var(--color-accent-ink)' : 'none') : 'none',
        opacity: dropped() && !accepts() ? 0.55 : 1,
      }}
      title={dropped() && !accepts() ? (dropped() as { reason: string }).reason : undefined}
      onDragOver={(e) => {
        if (accepts()) e.preventDefault();
      }}
      onDrop={(e) => {
        const source = ui.dragged();
        if (!source || !accepts()) return;
        e.preventDefault();
        ui.setDragged(null);
        ui.requestMove(source, props.lane);
      }}
    >
      <div class="flex min-w-0 flex-col gap-0.5">
        <Show
          when={isDone()}
          fallback={
            <h4 class="text-micro font-bold uppercase tracking-widest" style={{ 'overflow-wrap': 'anywhere', color: st().tokens.length > 0 && (st().key === 'needs-you' || st().key === 'review') ? `var(--run-${st().key === 'review' ? 'review' : 'needs'}-ink)` : 'var(--color-text-tertiary)' }}>
              {st().label}
              <span class="tabular-nums" style={{ 'margin-left': '0.4rem' }}>{st().tokens.length}</span>
            </h4>
          }
        >
          <h4 class="text-micro font-bold uppercase tracking-widest" style={{ color: 'var(--color-text-tertiary)' }}>
            <button
              type="button"
              data-testid="flow-done-toggle"
              aria-expanded={unfolded()}
              class="font-bold uppercase tracking-widest focus:outline-none focus-visible:ring-2"
              style={{ background: 'none', border: 0, padding: 0, cursor: 'pointer', color: 'inherit', 'font-size': 'inherit' }}
              onClick={() => setUnfolded((v) => !v)}
            >
              {`${unfolded() ? '▾' : '▸'} ${st().label}`}
              <span class="tabular-nums" style={{ 'margin-left': '0.4rem' }}>{st().tokens.length}</span>
            </button>
          </h4>
        </Show>
        <Show when={st().step}>
          {(step) => (
            <span class="text-caption" style={{ ...MUTED, 'overflow-wrap': 'anywhere' }}>
              {step().role ?? (step().kind === 'command' ? 'runs a command' : step().kind === 'ask' ? 'asks a person' : step().kind === 'wait' ? 'waits' : step().kind === 'parallel' ? 'runs side by side' : '')}
            </span>
          )}
        </Show>
        <Show when={st().key === 'working' && !props.lane.reportsSteps}>
          <span class="text-caption" style={FAINT}>The agent reports no steps</span>
        </Show>
        <Show when={st().key === 'working' && props.lane.reportsSteps}>
          <span class="text-caption" style={FAINT}>Step not reported for these runs</span>
        </Show>
        <Show when={sentence()}>
          {(text) => <span class="sr-only">{`Then ${text()}`}</span>}
        </Show>
      </div>
      <Show
        when={!isDone() || unfolded()}
        fallback={<Show when={st().tokens.length === 0}><span class="text-caption" style={FAINT}>Nothing ended today</span></Show>}
      >
        <ul aria-label={label()} class="flex min-w-0 flex-col gap-2">
          <For each={st().tokens} fallback={<li class="text-caption" style={FAINT}>{isUpNext() ? 'Nothing waiting' : 'None'}</li>}>
            {(token) => <Token token={token} source={sourceOf(token)} />}
          </For>
        </ul>
      </Show>
    </li>
  );
};

/**
 * One team's lane, or one single agent's: a header that says who it is and how it works, and below it
 * the places a task stands as it goes: Up next, the team's own steps (a check between two of them is
 * a small glyph), Needs you and Done today. A lane with nothing going on folds to its header.
 */
const FlowLaneView: Component<{ lane: FlowLane; limit: number | null }> = (props) => {
  const lane = () => props.lane;
  const [open, setOpen] = createSignal(lane().urgency !== 'idle');
  let touched = false;
  createEffect(() => {
    const wanted = lane().urgency !== 'idle';
    if (!touched) setOpen(wanted);
  });
  const members = () => lane().report?.members ?? null;
  const stations = createMemo(() => lane().stations);
  const href = () => (lane().kind === 'team' ? `/agents#team-${encodeURIComponent(lane().name)}` : '/agents');
  const total = () => lane().tokens.length;
  const tallyText = () => tallySentence(runTally({ needs: lane().counts.needs, review: lane().counts.review, failed: 0 }));

  return (
    <li
      data-testid="flow-lane"
      data-lane={lane().key}
      data-urgency={lane().urgency}
      class="flex min-w-0 flex-col gap-3 rounded-card px-4 py-3"
      style={{ 'background-color': 'var(--color-bg-panel)' }}
    >
      <div class="flex min-w-0 flex-wrap items-start gap-x-4 gap-y-2">
        <div class="flex min-w-0 flex-1 flex-col gap-1.5" style={{ 'flex-basis': '16rem' }}>
          <div class="flex min-w-0 flex-wrap items-center gap-2">
            <button
              type="button"
              aria-expanded={open()}
              aria-label={`${open() ? 'Fold' : 'Unfold'} the lane ${lane().name}`}
              data-testid="flow-lane-toggle"
              class="flex-none focus:outline-none focus-visible:ring-2"
              style={{ background: 'none', border: 0, padding: 0, cursor: 'pointer', color: 'var(--color-text-secondary)', 'border-radius': '6px' }}
              onClick={() => {
                touched = true;
                setOpen((v) => !v);
              }}
            >
              <span aria-hidden="true">{open() ? '▾' : '▸'}</span>
            </button>
            <h3 class="min-w-0 text-lead font-bold" style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}>
              <A
                href={href()}
                data-testid="flow-lane-name"
                class="hover:underline"
                title={lane().kind === 'team' ? `Open the card of the team ${lane().name}` : 'Open the Agents page'}
                style={{ color: 'inherit' }}
              >
                {lane().name}
                <span class="sr-only">{lane().kind === 'team' ? ` — open the team's card` : ' — open the Agents page'}</span>
              </A>
            </h3>
            <span class="text-caption tabular-nums" style={MUTED} data-testid="flow-lane-count">
              {`${total()} ${total() === 1 ? 'task' : 'tasks'}`}
              {tallyText() ? ` · ${tallyText()}` : ''}
            </span>
          </div>
          <Facts lane={lane()} />
          <Show when={members()}>
            {(list) => (
              <Show when={list().length > 0} fallback={<span class="text-caption" style={FAINT}>No members reported</span>}>
                <ul aria-label={`Members of ${lane().name}`} class="flex flex-wrap items-center" data-testid="flow-members">
                  <For each={list()}>{(member) => <MemberMark member={member} />}</For>
                </ul>
                <Show when={list().some((m) => m.writes_project === true || m.can_propose === true)}>
                  <span
                    data-testid="flow-members-legend"
                    class="text-caption"
                    style={FAINT}
                    title="A ring around a member means it changes the project; a dot on it means it can propose items."
                  >
                    ring: changes the project · dot: can propose
                  </span>
                </Show>
              </Show>
            )}
          </Show>
        </div>
        <div class="flex min-w-0 flex-col gap-2" style={{ 'flex-basis': '14rem' }}>
          <QueueMeter lane={lane()} />
          <TokenMeter lane={lane()} limit={props.limit} />
        </div>
      </div>
      <Show when={open()}>
        <div role="region" aria-label={`Stations of ${lane().name}`} tabindex="0" class="min-w-0 overflow-x-auto focus:outline-none focus-visible:ring-2">
          <ol class="flex min-w-0 items-stretch" style={{ width: '100%' }}>
            <For each={stations()}>
              {(station, i) => (
                <>
                  <Station lane={lane()} station={station} />
                  <Show when={station.check && i() < stations().length - 1}>
                    <li aria-hidden="true" data-testid="flow-check" data-check={station.check ?? undefined} class="flex flex-none items-center px-0.5" style={FAINT} title={station.step ? (checkText(station.step) ?? undefined) : undefined}>
                      {CHECK_GLYPH[station.check!]()}
                    </li>
                  </Show>
                </>
              )}
            </For>
          </ol>
        </div>
      </Show>
    </li>
  );
};

export default FlowLaneView;
