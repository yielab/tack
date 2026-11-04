import { type Component, type JSX, For, Show } from 'solid-js';
import { Badge, Button } from '../../shared/ui';
import {
  agentName,
  memberReachText,
  misfitText,
  webText,
  networkText,
  pausedText,
  plainName,
  queueText,
  teamProposes,
  SHARED_PROPOSALS_TEXT,
  stepText,
  supervisionText,
  type AgentCapabilityLine,
  type TeamEntry,
  type TeamStep,
} from '../../shared/execution';
import { CapabilityGlyph, IconBranch, IconFolder } from '../../shared/runWithAgent/runDialogIcons';
import { requestProfileEdit } from './profileEditRequest';
import DisabledLine from './DisabledLine';

const HAIRLINE = { 'border-top': '1px solid var(--color-border-light)' } as const;

// Glyphs are built fresh on every call: a DOM node can sit in one place only.
const svg = (body: JSX.Element): JSX.Element => (
  <svg
    width="14"
    height="14"
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    stroke-width="1.7"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    class="flex-none"
  >
    {body}
  </svg>
);
const IconEye = () =>
  svg(
    <>
      <path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z" />
      <circle cx="8" cy="8" r="2" />
    </>,
  );
const IconPencil = () => svg(<path d="M10.5 3l2.5 2.5L5.5 13H3v-2.5zM9 4.5L11.5 7" />);
const IconArrow = () => svg(<path d="M3 8h9M9 4.5L12.5 8 9 11.5" />);
const IconGlobe = () =>
  svg(
    <>
      <circle cx="8" cy="8" r="6" />
      <path d="M2 8h12M8 2c1.8 1.7 2.6 3.7 2.6 6S9.8 12.3 8 14C6.2 12.3 5.4 10.3 5.4 8S6.2 3.7 8 2z" />
    </>,
  );

/** One labelled part of a card: drawn from `value` when the agent reported it, else one disabled line
 *  saying who did not. `value` is false-y only for a field the agent left out (an empty list is a report). */
function Part<T>(props: {
  id: string;
  agent: string;
  value: T | null | undefined | false | '';
  label?: string;
  children: (value: NonNullable<T>) => JSX.Element;
}): JSX.Element {
  return (
    <div data-testid={`team-${props.id}`} class="flex min-w-0 flex-col gap-1.5" style={props.label ? HAIRLINE : undefined}>
      <Show when={props.label}>
        <span class="pt-2.5 text-micro font-bold uppercase tracking-widest" style={{ color: 'var(--color-text-tertiary)' }}>
          {props.label}
        </span>
      </Show>
      <Show when={props.value} fallback={<DisabledLine reported={false}>Not reported by {props.agent}</DisabledLine>}>
        {(v) => props.children(v() as NonNullable<T>)}
      </Show>
    </div>
  );
}

/** A labelled line of the rules block: what the team decides, its network, whether it proposes. */
function Rule<T>(props: { id: string; label: string; agent: string; value: T | null | undefined | false | ''; children: (value: NonNullable<T>) => JSX.Element }): JSX.Element {
  return (
    <div data-testid={`team-${props.id}`} class="flex flex-col gap-0.5 sm:flex-row sm:gap-3">
      <dt class="w-28 flex-none text-micro font-bold uppercase tracking-widest" style={{ color: 'var(--color-text-tertiary)' }}>
        {props.label}
      </dt>
      <dd class="min-w-0 text-small" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>
        <Show when={props.value} fallback={<DisabledLine reported={false}>Not reported by {props.agent}</DisabledLine>}>
          {(v) => props.children(v() as NonNullable<T>)}
        </Show>
      </dd>
    </div>
  );
}

/** An icon with the words a screen reader and a tooltip need. */
const Mark: Component<{ words: string; children: JSX.Element; attr?: string }> = (props) => (
  <span role="img" aria-label={props.words} title={props.words} data-mark={props.attr} class="inline-flex flex-none" style={{ color: 'var(--color-text-secondary)' }}>
    {props.children}
  </span>
);

/** The step as a node of the pipeline: its name, then what the agent said of it. A step that waits is dashed. */
const StepNode: Component<{ step: TeamStep; last: boolean }> = (props) => {
  const detail = () => {
    const text = stepText(props.step);
    return text.startsWith(props.step.name) ? text.slice(props.step.name.length).replace(/^ · /, '') : text;
  };
  return (
    <li data-kind={props.step.kind} class="flex min-w-0 items-center gap-1.5">
      <span
        class="flex min-w-0 flex-col rounded-item px-2.5 py-1.5 text-small"
        style={{
          border: `1px ${props.step.kind === 'wait' ? 'dashed' : 'solid'} var(--color-border-medium)`,
          'overflow-wrap': 'anywhere',
        }}
      >
        <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>{props.step.name}</span>
        <Show when={detail()}>
          <span class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>{detail()}</span>
        </Show>
      </span>
      <Show when={!props.last}>
        <span aria-hidden="true" style={{ color: 'var(--color-text-tertiary)' }}>
          <IconArrow />
        </span>
      </Show>
    </li>
  );
};

const Counter: Component<{ value: number; label: string }> = (props) => (
  <span class="flex items-baseline gap-1.5">
    <span class="text-lead font-bold leading-none" style={{ color: 'var(--color-text-primary)' }}>{props.value}</span>
    <span class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{props.label}</span>
  </span>
);

export interface TeamCardProps {
  entry: TeamEntry;
  /** What the team's agent reported of proposing, which a team inherits from its agent. */
  proposes: AgentCapabilityLine;
  busy: boolean;
  /** Another team's profile is being made: every "Make a profile" waits. */
  anyBusy: boolean;
  onMakeProfile: () => void;
}

/**
 * One team, drawn as a team: who runs it and how it is shaped at the top, its members as rows, its
 * steps as a pipeline, what it has waiting and running as two counters, the record sets it keeps, and
 * the profiles that use it. Every part is drawn only from a field the agent reported; a field it left
 * out is one disabled line, never hidden.
 */
const TeamCard: Component<TeamCardProps> = (props) => {
  const entry = () => props.entry;
  const agent = () => (entry().harness ? agentName(entry().harness!) : 'any computer');
  const report = () => entry().report ?? undefined;
  // What the team's own report says of proposing (each member's `can_propose`, as Compare reads it);
  // the agent's general ability is the fallback only where the report names no member, and then it
  // carries no more than the agent said.
  const proposal = (): { state: 'can' | 'cannot'; text: string; reason: string | null } | false => {
    const said = teamProposes(report());
    if (said.state === 'can') return { state: 'can', text: `Can propose items: ${said.roles.join(', ')}`, reason: props.proposes.state === 'can' ? props.proposes.reason : null };
    if (said.state === 'cannot') return { state: 'cannot', text: 'No member proposes items', reason: props.proposes.state === 'cannot' ? props.proposes.reason : null };
    if (props.proposes.state === 'not_reported') return false;
    return { state: props.proposes.state, text: props.proposes.state === 'can' ? 'Can propose items' : 'Cannot propose items', reason: props.proposes.reason };
  };

  return (
    <li id={`team-${entry().name}`} class="flex min-w-0 flex-col gap-2.5 rounded-xl bg-app px-4 py-3.5" data-testid={`team-card-${entry().name}`}>
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-lead font-bold" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>
          {entry().name}
        </span>
        <Show when={entry().reported} fallback={<Badge tone="neutral">Not reported by any computer</Badge>}>
          <Badge tone="primary">Run by {agentName(entry().harness ?? '')}</Badge>
        </Show>
        <Show when={entry().reported}>
          <Part id="shape" agent={agent()} value={report()?.shape}>
            {(shape) => <Badge tone="neutral">{shape}</Badge>}
          </Part>
          <Part id="works-in" agent={agent()} value={report()?.works_in}>
            {(where) => (
              <span class="inline-flex items-center gap-1.5 text-small" style={{ color: 'var(--color-text-secondary)' }}>
                {where === 'repository' ? <IconBranch /> : <IconFolder />}
                {where === 'repository' ? 'Works in this repository' : 'Works in a folder'}
                <Show when={where === 'folder' && report()?.folder}>
                  {(folder) => (
                    <span data-testid="team-folder" class="min-w-0 font-mono text-caption" style={{ 'overflow-wrap': 'anywhere' }}>
                      {folder()}
                    </span>
                  )}
                </Show>
              </span>
            )}
          </Part>
        </Show>
      </div>

      <Show when={entry().reported}>
        <Part id="members" label="Members" agent={agent()} value={report()?.members}>
          {(members) => (
            <ul class="flex flex-col">
              <For each={members} fallback={<DisabledLine>No members reported</DisabledLine>}>
                {(m) => {
                  // memberReachText says the reach first (when the agent reported it) and "Can propose items" last.
                  const words = memberReachText(m);
                  const reach = () => (m.writes_project == null ? null : words[0]);
                  const proposes = () => (m.can_propose === true ? words[words.length - 1] : null);
                  const web = () => webText(m);
                  return (
                    <li data-testid={`team-member-${m.role}`} class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 py-1.5 text-small">
                      <Badge tone="neutral">{m.role}</Badge>
                      <Show when={m.model}>
                        <span class="min-w-0 font-mono text-caption" style={{ color: 'var(--color-text-secondary)', 'overflow-wrap': 'anywhere' }}>{m.model}</span>
                      </Show>
                      <Show when={reach()}>
                        {(word) => (
                          <Mark words={word()} attr={m.writes_project ? 'changes' : 'reads'}>
                            {m.writes_project ? <IconPencil /> : <IconEye />}
                          </Mark>
                        )}
                      </Show>
                      <Show when={web()}>
                        {(word) => (
                          <Mark words={word()} attr="web">
                            <IconGlobe />
                          </Mark>
                        )}
                      </Show>
                      <Show when={proposes()}>
                        {(word) => (
                          <Mark words={word()} attr="proposes">
                            <CapabilityGlyph id="proposals" />
                          </Mark>
                        )}
                      </Show>
                    </li>
                  );
                }}
              </For>
            </ul>
          )}
        </Part>

        <Part id="steps" label="Steps" agent={agent()} value={report()?.steps}>
          {(steps) => (
            <ol aria-label="The team's steps" class="flex flex-wrap gap-y-2">
              <For each={steps} fallback={<DisabledLine>No steps reported</DisabledLine>}>
                {(step, i) => <StepNode step={step} last={i() === steps.length - 1} />}
              </For>
            </ol>
          )}
        </Part>

        <div class="grid min-w-0 gap-x-6 gap-y-2.5 sm:grid-cols-2">
          <Part id="queue" label="Work now" agent={agent()} value={report()?.queue}>
            {(queue) => (
              <div class="flex flex-wrap gap-x-5 gap-y-1">
                <span class="sr-only">{queueText(queue)}</span>
                <span aria-hidden="true" class="contents">
                  <Counter value={queue.waiting} label="waiting" />
                  <Counter value={queue.running} label="running" />
                </span>
              </div>
            )}
          </Part>
          <Part id="records" label="Keeps" agent={agent()} value={report()?.records}>
            {(sets) => (
              <ul class="flex flex-col gap-1">
                <For each={sets} fallback={<DisabledLine>Keeps no record sets</DisabledLine>}>
                  {(set) => (
                    <li class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 text-small" style={{ 'overflow-wrap': 'anywhere' }}>
                      <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>{set.name}</span>
                      <Badge tone="neutral">
                        {set.rows} {set.rows === 1 ? 'row' : 'rows'}
                      </Badge>
                      <Show when={set.description}>
                        <span style={{ color: 'var(--color-text-secondary)' }}>{set.description}</span>
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
            )}
          </Part>
        </div>

        <dl class="flex flex-col gap-1.5 pt-2.5" style={HAIRLINE}>
          <Rule id="supervision" label="Supervision" agent={agent()} value={supervisionText(report()?.profile)}>
            {(supervision) => supervision}
          </Rule>
          <Rule id="decides" label="Runs" agent={agent()} value={report()?.decides}>
            {(decides) => (
              <>
                <span>{decides === 'to_the_end' ? 'Runs to the end by itself' : 'Stops to ask a person'}</span>
                <For each={report()?.violations ?? []}>
                  {(v) => (
                    <p class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>
                      {v.requirement}. To fix: {v.fix}
                    </p>
                  )}
                </For>
              </>
            )}
          </Rule>
          <Rule id="network" label="Network" agent={agent()} value={report()?.network}>
            {(network) => networkText(network)}
          </Rule>
          <Rule id="proposals" label="Proposes" agent={agent()} value={proposal()}>
            {(line) => (
              <span
                aria-disabled={line.state === 'can' ? undefined : 'true'}
                style={{ color: line.state === 'can' ? undefined : 'var(--color-text-tertiary)' }}
              >
                {line.text}
                <Show when={line.reason}>
                  <span style={{ color: 'var(--color-text-tertiary)' }}> · {plainName(line.reason!)}</span>
                </Show>
              </span>
            )}
          </Rule>
        </dl>
        <Show when={report()?.paused}>
          {(paused) => (
            <p data-testid="team-paused" class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>
              <strong style={{ color: 'var(--color-text-primary)' }}>{pausedText(entry().name, paused()).headline}.</strong>{' '}
              {pausedText(entry().name, paused()).resume}
            </p>
          )}
        </Show>
        <Show when={report()?.proposals_shared_across_tasks === true}>
          <p data-testid="team-proposals-shared" class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>
            {SHARED_PROPOSALS_TEXT}
          </p>
        </Show>
      </Show>

      <div class="flex flex-wrap items-center gap-x-2 gap-y-1.5 pt-2.5 text-small" style={{ ...HAIRLINE, color: 'var(--color-text-secondary)' }}>
        <Show when={entry().profiles.length > 0} fallback={<span>No profile uses this team yet.</span>}>
          <span>Profiles that use it:</span>
          <ul class="flex flex-wrap gap-x-3 gap-y-1" data-testid="team-profiles">
            <For each={entry().profiles}>
              {(p) => (
                <li class="min-w-0">
                  <button
                    type="button"
                    class="text-left font-semibold underline"
                    style={{ color: 'var(--color-accent-ink)', 'overflow-wrap': 'anywhere' }}
                    onClick={() => requestProfileEdit(p.id)}
                  >
                    {p.name}
                  </button>
                </li>
              )}
            </For>
          </ul>
          <For each={entry().profiles.flatMap((p) => (p.misfits ?? []).map((misfit) => ({ name: p.name, misfit })))}>
            {(line) => (
              <p data-testid="team-misfit" class="w-full text-caption" style={{ color: 'var(--color-text-secondary)', 'overflow-wrap': 'anywhere' }}>
                <strong style={{ color: 'var(--color-text-primary)' }}>{line.name} is refused here:</strong> it{' '}
                {misfitText(entry().name, line.misfit)}
              </p>
            )}
          </For>
        </Show>
        <Show when={entry().reported && entry().harness && entry().profiles.length === 0}>
          <Button type="button" size="sm" variant="secondary" loading={props.busy} disabled={props.anyBusy} onClick={props.onMakeProfile}>
            Make a profile for this team
          </Button>
        </Show>
      </div>
    </li>
  );
};

export default TeamCard;
