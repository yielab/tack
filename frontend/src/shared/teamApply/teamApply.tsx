import { type Accessor, type Component, For, Show, createResource, createSignal } from 'solid-js';
import { applyTeams, type TeamToApply } from '../api/teams';
import { gateApiFeature, runnersApi, type CapabilityGate } from '../execution';
import { isAnsweringRunner, runnerSummaryToCapabilities } from '../runWithAgent/shared';
import Button from '../ui/Button';
import Modal from '../ui/Modal';

/** The shapes docket makes a team from; a missing team's shape is where the file or the person says so. */
const TEAM_SHAPES = ['build', 'respond', 'operate', 'routine'] as const;

/** The shape a team gets when nothing said which: the one every project has. */
const DEFAULT_SHAPE = 'build';

interface Note {
  team: string;
  state: 'applied' | 'refused' | 'failed' | 'declined';
  text: string;
}

interface Review {
  team: string;
  shape: string;
  code: string;
  commands: string[];
}

export interface TeamApply {
  /** Whether the board may ask a runner to make teams for this project, and the reason when it may not. */
  gate: Accessor<CapabilityGate>;
  /** Teams a call is making now; while any is, no other call starts. */
  making: Accessor<readonly string[]>;
  shapeOf: (team: string) => string;
  chooseShape: (team: string, shape: string) => void;
  /** Makes the teams; one docket waits on a person to read goes to the dialog, and the rest are noted. */
  apply: (teams: TeamToApply[]) => Promise<void>;
  /** What came of the last calls, and the dialog that shows the commands docket wants read. Drawn once. */
  Host: Component;
}

/**
 * Making a project's missing teams through a runner that has docket: one call per press (up to 30 seconds a
 * team), the answer for each team as a line, and for a team whose folder would run commands a dialog that
 * lists them and sends the team again with docket's code once the person accepts.
 */
export function createTeamApply(options: {
  projectId: Accessor<string | undefined>;
  /** Whether anything on screen needs the answer; the computers are not asked until it does. */
  active?: Accessor<boolean>;
  /** What to say when there is no project to make a team in. */
  noProject?: string;
  /** Called when at least one team was made, so the screen can read the teams again. */
  onApplied?: () => void;
}): TeamApply {
  const [runners] = createResource(
    () => (options.projectId() && (options.active?.() ?? true) ? options.projectId() : undefined),
    () => runnersApi.list().then((r) => r.data.data ?? [], () => []),
  );
  const gate = (): CapabilityGate => {
    if (!options.projectId()) return { enabled: false, reason: options.noProject ?? 'Choose a project to make its teams in.' };
    const snapshots = (runners() ?? []).filter(isAnsweringRunner).flatMap((r) => runnerSummaryToCapabilities(r) ?? []);
    return gateApiFeature(snapshots, 'docket', 'podShape');
  };

  const [making, setMaking] = createSignal<readonly string[]>([]);
  const [notes, setNotes] = createSignal<Note[]>([]);
  const [reviews, setReviews] = createSignal<Review[]>([]);
  const [shapes, setShapes] = createSignal<Record<string, string>>({});

  const apply = async (teams: TeamToApply[]) => {
    const id = options.projectId();
    if (!id || making().length > 0 || !gate().enabled || teams.length === 0) return;
    const names = teams.map((t) => t.name);
    setMaking(names);
    setNotes((all) => all.filter((n) => !names.includes(n.team)));
    const fresh: Note[] = [];
    try {
      const results = await applyTeams(id, teams);
      for (const result of results) {
        const asked = teams.find((t) => t.name === result.team);
        if (result.state === 'applied') fresh.push({ team: result.team, state: 'applied', text: `${result.team} is made and its folder applied.` });
        else if (result.state === 'refused') fresh.push({ team: result.team, state: 'refused', text: result.message || `${result.team} was refused.` });
        else if (result.review && asked) {
          const { code, commands } = result.review;
          setReviews((all) => [...all, { team: result.team, shape: asked.shape, code, commands }]);
        }
      }
      if (results.some((r) => r.state === 'applied')) options.onApplied?.();
    } catch (err) {
      for (const name of names) fresh.push({ team: name, state: 'failed', text: err instanceof Error ? err.message : 'The teams could not be made.' });
    } finally {
      setMaking([]);
      setNotes((all) => [...all, ...fresh]);
    }
  };

  const leaveReview = (review: Review) => setReviews((all) => all.filter((r) => r !== review));
  const accept = (review: Review) => {
    leaveReview(review);
    void apply([{ name: review.team, shape: review.shape, accept: review.code }]);
  };
  const decline = (review: Review) => {
    leaveReview(review);
    setNotes((all) => [...all, { team: review.team, state: 'declined', text: `${review.team} is not made: its commands were not accepted.` }]);
  };

  const Host: Component = () => (
    <>
      <Show when={notes().length > 0}>
        <ul data-testid="team-apply-notes" aria-label="What came of making the teams" class="flex min-w-0 flex-col gap-1 text-small">
          <For each={notes()}>
            {(note) => (
              <li
                data-state={note.state}
                role={note.state === 'applied' ? 'status' : 'alert'}
                style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}
              >
                {note.state === 'refused' || note.state === 'failed' ? `${note.team}: ${note.text}` : note.text}
              </li>
            )}
          </For>
        </ul>
      </Show>
      <Show when={reviews()[0]}>
        {(review) => (
          <Modal isOpen onClose={() => decline(review())} title={`Read what ${review().team} would run`} size="md">
            <div class="flex flex-col gap-3 text-sm" data-testid="team-apply-review" style={{ color: 'var(--color-text-secondary)' }}>
              <p>
                The folder <code class="font-mono">.docket/teams/{review().team}</code> would run these commands on your computer when docket
                applies it. Apply it only if you trust them.
              </p>
              <ul aria-label={`Commands ${review().team} would run`} class="flex flex-col gap-1">
                <For each={review().commands}>
                  {(command) => (
                    <li>
                      <code class="font-mono text-caption" style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}>{command}</code>
                    </li>
                  )}
                </For>
              </ul>
              <div class="flex justify-end gap-2">
                <Button type="button" variant="secondary" onClick={() => decline(review())}>Cancel</Button>
                <Button type="button" onClick={() => accept(review())}>Accept and apply</Button>
              </div>
            </div>
          </Modal>
        )}
      </Show>
    </>
  );

  return {
    gate,
    making,
    shapeOf: (team) => shapes()[team] ?? DEFAULT_SHAPE,
    chooseShape: (team, shape) => setShapes((all) => ({ ...all, [team]: shape })),
    apply,
    Host,
  };
}

/** Which shape a team is made from, for a screen that does not know it from a file. */
export const ShapeSelect: Component<{ name: string; teams: TeamApply }> = (props) => (
  <select
    aria-label={`Shape of ${props.name}`}
    class="rounded-full border px-3 py-1 text-xs"
    style={{ 'background-color': 'var(--color-bg-base)', color: 'var(--color-text-primary)', 'border-color': 'var(--color-border-medium)' }}
    onChange={(e) => props.teams.chooseShape(props.name, e.currentTarget.value)}
  >
    <For each={TEAM_SHAPES}>{(s) => <option value={s} selected={s === props.teams.shapeOf(props.name)}>{s}</option>}</For>
  </select>
);

/**
 * Makes one missing team beside the command that would. A shape the file gave is used as it is; a screen
 * that does not know it (the Map is worked out from profiles, not from a file) asks for it.
 */
export const CreateTeam: Component<{ name: string; shape?: string | null; teams: TeamApply }> = (props) => {
  const shape = () => props.shape || props.teams.shapeOf(props.name);
  const busy = () => props.teams.making().includes(props.name);
  return (
    <span class="inline-flex min-w-0 flex-wrap items-start gap-2" data-testid="create-team">
      <Show when={!props.shape}>
        <ShapeSelect name={props.name} teams={props.teams} />
      </Show>
      <Button
        type="button"
        size="sm"
        variant="secondary"
        aria-label={`Create the team ${props.name}`}
        loading={busy()}
        disabled={!props.teams.gate().enabled || props.teams.making().length > 0}
        reason={props.teams.gate().reason ?? undefined}
        onClick={() => void props.teams.apply([{ name: props.name, shape: shape() }])}
      >
        Create
      </Button>
    </span>
  );
};
