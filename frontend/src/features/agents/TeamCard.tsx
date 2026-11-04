import { type Component, type JSX, For, Show, createUniqueId } from 'solid-js';
import { Badge, Button } from '../../shared/ui';
import { BOOK } from '../../shared/help/routes';
import type { components } from '../../shared/api/schema.gen';
import {
  agentName,
  deliversText,
  firstLine,
  isTeamOfOne,
  memberReachText,
  methodsByType,
  misfitText,
  webText,
  networkText,
  pausedText,
  plainName,
  queueText,
  runnableFirst,
  teamProposes,
  SHARED_PROPOSALS_TEXT,
  stepText,
  supervisionText,
  type AgentCapabilityLine,
  type AgentProfileSummary,
  type TeamEntry,
  type TeamReport,
  type TeamStep,
} from '../../shared/execution';
import { CapabilityGlyph, IconBranch, IconFolder } from '../../shared/runWithAgent/runDialogIcons';
import { requestProfileEdit } from './profileEditRequest';
import DisabledLine from './DisabledLine';
import TeamMemory from './TeamMemory';

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

type TeamLine = components['schemas']['TeamLine'];

/** What a chosen project says of a team: how far Tack lets it go and which roles send it work. */
export interface TeamSheetContext {
  project: string;
  /** The most autonomy the project can reach now; absent when the board did not say. */
  ceiling: string | null;
  /** This team's line of the project's autonomy settings; null when no role of the project uses it. */
  line: TeamLine | null;
  roles: ReadonlyArray<{ name: string; profileId: string | null }>;
  defaultProfileId: string | null;
}

export interface TeamCardProps {
  entry: TeamEntry;
  /** What the team's agent reported of proposing, which a team inherits from its agent. */
  proposes: AgentCapabilityLine;
  busy: boolean;
  /** Another team's profile is being made: every "Make a profile" waits. */
  anyBusy: boolean;
  onMakeProfile: () => void;
  /** Every profile of the board, for the method each names per kind of card. */
  profiles: readonly AgentProfileSummary[];
  /** The chosen project's view of the team; null while no project is chosen. */
  context: TeamSheetContext | null;
  /** Buttons that act on the team, drawn in the sheet's footer beside "Make a profile". */
  actions?: JSX.Element;
  /** The team's report as its agent gave it after a person acted on it from the sheet (a memory entry decided). */
  onReport?: (report: TeamReport | null) => void;
}

/** One of the six questions of a team's sheet, with its answer. */
const Question: Component<{ id: string; title: string; children: JSX.Element }> = (props) => {
  const heading = createUniqueId();
  return (
    <section data-testid={`team-q-${props.id}`} aria-labelledby={heading} class="flex min-w-0 flex-col gap-2 pt-2.5" style={HAIRLINE}>
      <h4 id={heading} class="text-small font-bold" style={{ color: 'var(--color-text-primary)' }}>
        {props.title}
      </h4>
      {props.children}
    </section>
  );
};

const MUTED = { color: 'var(--color-text-secondary)' } as const;

/**
 * One team, drawn as a sheet that answers six questions of any team, whether it has several members or is
 * one agent counted as a team: what it is for, who is in it, what happens to a card, when it asks, how far
 * it goes and how to send it work. Every line is drawn only from a field the agent reported; a field it left
 * out is one disabled line, never hidden. The footer holds what a person does to the team.
 */
const TeamCard: Component<TeamCardProps> = (props) => {
  const entry = () => props.entry;
  const agent = () => (entry().harness ? agentName(entry().harness!) : 'any computer');
  const report = () => entry().report ?? undefined;
  const solo = () => isTeamOfOne(entry());
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
  /** The profiles that name this team, with the method each gives to kinds of card. */
  const byType = () => {
    const mine = new Set(entry().profiles.map((p) => p.id));
    return props.profiles
      .filter((p) => mine.has(p.agent_profile_id))
      .flatMap((p) => methodsByType(p.tool_policy).map((m) => ({ profile: p.name, ...m })));
  };
  const defaultMethod = () => report()?.methods?.find((m) => m.default)?.name;
  /** The roles of the chosen project whose profile names this team. */
  const sendingRoles = () => {
    const ctx = props.context;
    if (!ctx) return [];
    const mine = new Map(entry().profiles.map((p) => [p.id, p.name]));
    const roles = ctx.roles
      .filter((r) => r.profileId && mine.has(r.profileId))
      .map((r) => ({ role: r.name, profile: mine.get(r.profileId!)! }));
    return ctx.defaultProfileId && mine.has(ctx.defaultProfileId)
      ? [...roles, { role: 'A task with no role', profile: mine.get(ctx.defaultProfileId)! }]
      : roles;
  };
  const reads = () => {
    const paths = report()?.read_only;
    if (!paths) return null;
    return paths.length
      ? { text: `Its sandbox also reads ${paths.join(', ')}, and cannot change them.`, grant: false }
      : { text: 'Its sandbox reads only its own work. A team that runs tests needs its toolchain granted.', grant: true };
  };

  return (
    <li id={`team-${entry().name}`} class="flex min-w-0 flex-col gap-2.5 rounded-xl bg-app px-4 py-3.5" data-testid={`team-card-${entry().name}`}>
      <div class="flex flex-wrap items-center gap-2">
        <h3 class="text-lead font-bold" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>
          {entry().name}
        </h3>
        <Show when={entry().reported} fallback={<Badge tone="neutral">Not reported by any computer</Badge>}>
          <Badge tone="primary">Run by {agentName(entry().harness ?? '')}</Badge>
        </Show>
        <Show when={solo()}>
          <Badge tone="neutral">A team of one</Badge>
        </Show>
      </div>

      <Show when={entry().reported}>
        <Question id="purpose" title="What is it for?">
          <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
            <Show when={!solo()}>
              <Part id="shape" agent={agent()} value={report()?.shape}>
                {(shape) => <Badge tone="neutral">{shape}</Badge>}
              </Part>
              <Part id="works-in" agent={agent()} value={report()?.works_in}>
                {(where) => (
                  <span class="inline-flex items-center gap-1.5 text-small" style={MUTED}>
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
          <Part id="delivers" agent={agent()} value={report()?.delivers}>
            {(delivers) => (
              <span class="text-small" style={{ color: 'var(--color-text-primary)' }}>
                {deliversText(delivers)}
              </span>
            )}
          </Part>
          <Show when={!solo()}>
            <Part id="records" agent={agent()} value={report()?.records}>
              {(sets) => (
                <ul class="flex flex-col gap-1">
                  <For each={sets} fallback={<DisabledLine>Keeps no record sets</DisabledLine>}>
                    {(set) => (
                      <li class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 text-small" style={{ 'overflow-wrap': 'anywhere' }}>
                        <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>Keeps {set.name}</span>
                        <Badge tone="neutral">
                          {set.rows} {set.rows === 1 ? 'row' : 'rows'}
                        </Badge>
                        <Show when={set.description}>
                          <span style={MUTED}>{set.description}</span>
                        </Show>
                      </li>
                    )}
                  </For>
                </ul>
              )}
            </Part>
          </Show>
        </Question>

        <Question id="members" title="Who is in it?">
          <Show
            when={!solo()}
            fallback={
              <p data-testid="team-members" class="text-small" style={{ color: 'var(--color-text-primary)' }}>
                One agent, <code class="font-mono">{entry().name}</code>
                <span style={MUTED}> · {report()?.members?.[0]?.model ? `model ${report()?.members?.[0]?.model}` : 'model chosen for each task'}</span>
              </p>
            }
          >
            <Part id="members" agent={agent()} value={report()?.members}>
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
          </Show>
        </Question>

        <Question id="method" title="What happens to a card?">
          <Part id="methods" agent={agent()} value={report()?.methods}>
            {(methods) => (
              <ul class="flex flex-col gap-1">
                <For each={runnableFirst(methods)} fallback={<DisabledLine>No methods reported</DisabledLine>}>
                  {(method) => (
                    <li data-testid={`team-method-${method.name}`} class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 text-small" style={{ 'overflow-wrap': 'anywhere' }}>
                      <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                        {method.name}
                      </span>
                      <Show when={method.default}>
                        <Badge tone="primary">{solo() ? "The agent's own loop" : 'Default'}</Badge>
                      </Show>
                      <Show when={method.error}>
                        <span data-testid="team-method-broken" style={{ color: 'var(--color-warning-700)' }}>
                          Does not load: {firstLine(method.error!)}
                        </span>
                      </Show>
                      <Show when={!method.error && method.missing_roles && method.missing_roles.length > 0}>
                        <span data-testid="team-method-missing" style={{ color: 'var(--color-warning-700)' }}>
                          Cannot run: no member plays {method.missing_roles!.join(', ')}
                        </span>
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
            )}
          </Part>
          <Show when={byType().length > 0}>
            <ul data-testid="team-by-type" aria-label="The method each kind of card goes through" class="flex flex-col gap-0.5 text-small" style={MUTED}>
              <For each={byType()}>
                {(row) => (
                  <li>
                    A {row.type} goes through <strong style={{ color: 'var(--color-text-primary)' }}>{row.method}</strong> ({row.profile})
                  </li>
                )}
              </For>
            </ul>
          </Show>
          <Show
            when={!solo()}
            fallback={
              <p data-testid="team-steps" class="text-small" style={MUTED}>
                The agent works through the task with its own loop, so there are no steps to draw.
              </p>
            }
          >
            <Part id="steps" agent={agent()} value={report()?.steps}>
              {(steps) => (
                <>
                  <Show when={defaultMethod()}>
                    <span class="text-caption" style={MUTED}>Steps of {defaultMethod()}, the method a card goes through when it names none</span>
                  </Show>
                  <ol aria-label="The team's steps" class="flex flex-wrap gap-y-2">
                    <For each={steps} fallback={<DisabledLine>No steps reported</DisabledLine>}>
                      {(step, i) => <StepNode step={step} last={i() === steps.length - 1} />}
                    </For>
                  </ol>
                </>
              )}
            </Part>
          </Show>
        </Question>

        <Question id="asks" title="When does it ask?">
          <dl class="flex flex-col gap-1.5">
            <Show
              when={!solo()}
              fallback={
                <>
                  <Rule id="model" label="Model" agent={agent()} value={report()?.members?.[0]?.model ?? 'Chosen by the agent for each task'}>
                    {(model) => model}
                  </Rule>
                  <p class="text-caption" style={MUTED}>
                    An agent on its own asks a person only when the run has "Ask before each action" on.
                  </p>
                </>
              }
            >
              <Rule id="supervision" label="Supervision" agent={agent()} value={supervisionText(report()?.profile)}>
                {(supervision) => supervision}
              </Rule>
              <Rule id="decides" label="Runs" agent={agent()} value={report()?.decides}>
                {(decides) => (
                  <>
                    <span>{decides === 'to_the_end' ? 'Runs to the end by itself' : 'Stops to ask a person'}</span>
                    <For each={report()?.violations ?? []}>
                      {(v) => (
                        <p class="text-caption" style={MUTED}>
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
            </Show>
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
          <Show when={(report()?.needs ?? []).length > 0}>
            <ul data-testid="team-needs" aria-label="What the team's work needs" class="flex flex-col gap-1 text-small">
              <For each={report()?.needs ?? []}>
                {(need) => (
                  <li data-satisfied={need.satisfied ? 'yes' : 'no'} style={{ 'overflow-wrap': 'anywhere', color: need.satisfied ? 'var(--color-text-secondary)' : 'var(--color-text-primary)' }}>
                    <Show when={need.satisfied} fallback={<strong>The operator still has to: {need.name}.</strong>}>
                      Ready: {need.name}.
                    </Show>
                    <Show when={!need.satisfied && need.fix}>
                      <span style={MUTED}> {need.fix}</span>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
          </Show>
          <Show when={report()?.proposals_shared_across_tasks === true}>
            <p data-testid="team-proposals-shared" class="text-caption" style={MUTED}>
              {SHARED_PROPOSALS_TEXT}
            </p>
          </Show>
          <Show when={!solo()}>
            <Part id="memory" agent={agent()} value={report()?.memory}>
              {(memory) => (
                <TeamMemory
                  team={entry().name}
                  memory={memory}
                  deciding={{ harness: entry().harness ?? null, canDecide: memory.can_decide === true, onDecided: (next) => props.onReport?.(next) }}
                />
              )}
            </Part>
          </Show>
        </Question>

        <Question id="far" title="How far does it go?">
          <dl class="flex flex-col gap-1.5">
            <Rule id="limit" label="The run" agent={agent()} value={entry().harness}>
              {(harness) => (
                <span>
                  A token limit from the profile, the run or the project.{' '}
                  {harness === 'docket' && !solo() ? 'The team applies it to the whole task.' : 'Tack applies it to the agent.'}
                </span>
              )}
            </Rule>
            <Show when={!solo()}>
              <Rule id="step-limit" label="A step" agent={agent()} value={true}>
                {() => <span>Its rounds, tool calls and time are the team's own guards. A step that runs out says where to raise them.</span>}
              </Rule>
            </Show>
          </dl>
          <Show
            when={props.context}
            fallback={<DisabledLine testid="team-ceiling-none">Choose a project above to see how far Tack lets this team go in it.</DisabledLine>}
          >
            {(ctx) => (
              <div class="flex flex-col gap-0.5 text-small" data-testid="team-ceiling">
                <Show when={ctx().line} fallback={<span style={MUTED}>No role of {ctx().project} uses this team yet, so it holds the project back from nothing.</span>}>
                  {(line) => (
                    <span style={{ color: 'var(--color-text-primary)' }}>
                      {line().caps_at ? `In ${ctx().project}, its tasks stay at ${line().caps_at}: a person merges.` : `In ${ctx().project}, it runs lights-out.`}
                      <Show when={line().fix}>
                        <span style={MUTED}> To fix: {line().fix}</span>
                      </Show>
                    </span>
                  )}
                </Show>
                <Show when={ctx().ceiling}>
                  <span style={MUTED}>{ctx().project} can reach {ctx().ceiling} at most.</span>
                </Show>
              </div>
            )}
          </Show>
          <Show when={!solo()}>
            <Part id="queue" agent={agent()} value={report()?.queue}>
              {(queue) => (
                <div class="flex flex-wrap items-baseline gap-x-5 gap-y-1">
                  <span class="text-micro font-bold uppercase tracking-widest" style={{ color: 'var(--color-text-tertiary)' }}>Work now</span>
                  <span class="sr-only">{queueText(queue)}</span>
                  <span aria-hidden="true" class="contents">
                    <Counter value={queue.waiting} label="waiting" />
                    <Counter value={queue.running} label="running" />
                  </span>
                </div>
              )}
            </Part>
          </Show>
          <Show when={reads()}>
            {(line) => (
              <p data-testid="team-reads" class="text-caption" style={MUTED}>
                {line().text}
                <Show when={line().grant}>
                  {' '}
                  <a href={BOOK.toolchain} target="_blank" rel="noreferrer" class="underline">The Administrator Guide shows how.</a>
                </Show>
              </p>
            )}
          </Show>
          <Show when={report()?.paused}>
            {(paused) => (
              <p data-testid="team-paused" class="text-caption" style={MUTED}>
                <strong style={{ color: 'var(--color-text-primary)' }}>{pausedText(entry(), paused()).headline}.</strong>{' '}
                {pausedText(entry(), paused()).resume}
              </p>
            )}
          </Show>
        </Question>
      </Show>

      <Question id="send" title="How do I send it work?">
        <Show when={props.context}>
          <Show
            when={sendingRoles().length > 0}
            fallback={<p data-testid="team-roles" class="text-small" style={MUTED}>No role of {props.context!.project} sends work to this team.</p>}
          >
            <ul data-testid="team-roles" aria-label={`The roles of ${props.context!.project} that send work to this team`} class="flex flex-col gap-0.5 text-small">
              <For each={sendingRoles()}>
                {(row) => (
                  <li style={{ color: 'var(--color-text-primary)' }}>
                    <strong>{row.role}</strong> <span style={MUTED}>through the profile {row.profile}</span>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </Show>
        <Show when={!props.context}>
          <DisabledLine testid="team-roles-none">Choose a project above to see which of its roles send work to this team.</DisabledLine>
        </Show>
        <div class="flex flex-wrap items-center gap-x-2 gap-y-1.5 text-small" style={MUTED}>
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
                  {misfitText(entry().name, line.misfit, report())}
                </p>
              )}
            </For>
          </Show>
        </div>
      </Question>

      {/* Footer: what a person does to the team. Further buttons go in `actions`. */}
      <div data-testid="team-actions" class="flex flex-wrap items-center gap-2 pt-2.5" style={HAIRLINE}>
        <Show when={entry().reported && entry().harness && entry().profiles.length === 0}>
          <Button type="button" size="sm" variant="secondary" loading={props.busy} disabled={props.anyBusy} onClick={props.onMakeProfile}>
            Make a profile for this team
          </Button>
        </Show>
        {props.actions}
      </div>
    </li>
  );
};

export default TeamCard;
