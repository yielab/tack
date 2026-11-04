import { readRunSteps } from '../execution/attempts';
import type { RunLimit } from '../execution/api';
import type { RunStepBudget } from '../execution/types';
import type { TeamBudget, TeamReport } from '../execution/teams';
import { spaced } from './runLimit';
import { relativeTimeFromIso } from './shared';

/** One level of limit on a run, with the words that say who applies it. */
export interface LimitRow {
  id: 'run' | 'step' | 'team';
  title: string;
  /** What the limit is and where it stands, one fact a line. */
  lines: string[];
  /** Who applies the limit, so a person knows where to change it. */
  owner: string;
}

/** The team a run went to, as its agent reports it: only what limits need. */
export interface RunTeam {
  name: string;
  budget?: TeamBudget | null;
  paused?: TeamReport['paused'];
}

const BOUND_WORDS: Record<string, { title: string; unit: string }> = {
  rounds: { title: 'Rounds', unit: '' },
  toolCalls: { title: 'Tool calls', unit: '' },
  time: { title: 'Time', unit: ' seconds' },
};

const SOURCE_WORDS: Record<string, string> = {
  step: "set on the team's pipeline step",
  role: "set on the member's role",
  pod: "set on the team",
  task: "set on this task",
  default: "docket's default",
};

const WINDOW_WORDS: Record<string, string> = { lifetime: 'in all', day: 'each day' };

/** One step bound that ran out: "Rounds: 30 of 30", where it was set and where to raise it. */
function stepLines(budget: RunStepBudget): string[] | null {
  const bound = BOUND_WORDS[budget.bound];
  // A tokens stop is the run's limit, shown on the run row.
  if (!bound) return null;
  const lines = [`${bound.title}: ${spaced(budget.used)} of ${spaced(budget.limit)}${bound.unit}`];
  const source = SOURCE_WORDS[budget.source];
  if (source) lines.push(`Limit ${source}`);
  if (budget.raise) lines.push(`Raise it with ${budget.raise}`);
  return lines;
}

/**
 * The limits a run has, one row per level, each only when its agent said it: the run's tokens
 * (from the server, which owns the sentence and the source), the step bounds a step ran out of
 * (docket's hops) and the team's budget (docket's report). A run on an agent that has no teams and
 * no steps shows the run row alone.
 */
export function limitRows(input: { limit?: RunLimit | null; terminalReason?: unknown; team?: RunTeam | null }): LimitRow[] {
  const rows: LimitRow[] = [];
  const { limit, team } = input;
  if (limit) {
    const lines = [limit.sentence];
    if (limit.used != null && limit.tokens != null) lines.push(`${spaced(limit.used)} used so far`);
    rows.push({
      id: 'run',
      title: 'This run',
      lines,
      owner: limit.enforced_by === 'docket' ? 'Applied by docket' : "Applied by Tack's runner",
    });
  }
  const stepped = (readRunSteps(input.terminalReason) ?? [])
    .map((step) => step.budget)
    .filter((budget): budget is RunStepBudget => budget !== null)
    .map(stepLines)
    .filter((lines): lines is string[] => lines !== null);
  if (stepped.length > 0) {
    rows.push({ id: 'step', title: 'Each step', lines: stepped.flat(), owner: 'Applied by docket on its machine' });
  }
  const budget = team?.budget;
  if (team && budget) {
    const lines = [
      budget.tokens == null ? 'No token limit' : `${spaced(budget.tokens)} tokens ${WINDOW_WORDS[budget.window] ?? budget.window}`,
      `${spaced(budget.used)} used`,
    ];
    if (budget.window_start) lines.push(`Counting since ${relativeTimeFromIso(budget.window_start)}`);
    if (team.paused === 'budget') lines.push('Paused: the budget is spent');
    rows.push({ id: 'team', title: `Team ${team.name}`, lines, owner: 'Applied by docket' });
  }
  return rows;
}

/** The fold's one line while it is shut: which levels said something. */
export function limitsSummary(rows: readonly LimitRow[]): string {
  return rows.map((row) => (row.id === 'run' ? 'Run' : row.id === 'step' ? 'Step' : 'Team')).join(' · ');
}
