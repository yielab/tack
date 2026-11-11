// The steps a run's team reports, matched against the run's own events. Pure: no fetching, no
// rendering. The only events that name a step are `agent.hop_started` ("<Role> (<model>) started")
// and `agent.hop_finished` ("<Role> (<model>) <verdict> cycle N gate G"); a step is found by its
// role, case-insensitively. A step with no role (a command, a question, a wait, a parallel step) has
// no event to match, so it never gets a state: a state nothing can tell is not claimed.

import type { TeamStep } from '../execution/teams';
import type { TeamStepReport, TeamStepState } from '../execution/types';

type EventLike = { kind: string; payload: unknown };

const HOP_STARTED = 'agent.hop_started';
const HOP_FINISHED = 'agent.hop_finished';

/** The role an event's text opens with: "Lead (model) started" is "lead"; null when it names none. */
function roleOf(event: EventLike): string | null {
  const text = (event.payload as { text?: unknown } | null | undefined)?.text;
  if (typeof text !== 'string') return null;
  const found = /^(.+?) \(/.exec(text.trim());
  return found ? found[1].toLowerCase() : null;
}

/** What decides a step passed, in words; null when nothing does. */
export function checkText(step: TeamStep): string | null {
  const max = step.sends_back_max;
  const times = max == null ? '' : `, up to ${max} ${max === 1 ? 'time' : 'times'}`;
  switch (step.checked_by) {
    case 'reviewer':
      return step.sends_back_to ? `a reviewer can send it back${times}` : 'checked by a reviewer';
    case 'command':
      return step.sends_back_to ? `checked by a command, which can send it back${times}` : 'checked by a command';
    case 'person':
      return 'asks you';
    case 'external':
      return 'checked outside the team';
    default:
      return step.kind === 'ask' ? 'asks you' : null;
  }
}

/**
 * The reported steps in order, each with where it stands: `waiting` until its role starts, `running`
 * from the start until the matching end, `done` after. A role that starts again after it ended (the
 * work was sent back) is running again, and every later step waits again. An end with no start seen
 * still marks its step done.
 */
export function matchSteps(steps: readonly TeamStep[], events: readonly EventLike[]): TeamStepReport[] {
  const roles = steps.map((s) => (s.role ? s.role.toLowerCase() : null));
  const state: Array<TeamStepState | null> = roles.map((r) => (r === null ? null : 'waiting'));
  const indexesOf = (role: string) => roles.flatMap((r, i) => (r === role ? [i] : []));

  for (const event of events) {
    if (event.kind !== HOP_STARTED && event.kind !== HOP_FINISHED) continue;
    const role = roleOf(event);
    if (role === null) continue;
    const mine = indexesOf(role);
    if (mine.length === 0) continue;
    if (event.kind === HOP_STARTED) {
      const fresh = mine.find((i) => state[i] === 'waiting');
      const pick = fresh ?? mine[0];
      if (fresh === undefined) {
        for (let i = pick + 1; i < state.length; i++) if (state[i] !== null) state[i] = 'waiting';
      }
      state[pick] = 'running';
    } else {
      const pick = mine.find((i) => state[i] === 'running') ?? mine.find((i) => state[i] === 'waiting');
      if (pick !== undefined) state[pick] = 'done';
    }
  }

  return steps.map((s, i) => ({ name: s.name, kind: s.kind, role: s.role ?? null, state: state[i], check: checkText(s) }));
}
