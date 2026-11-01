import type { DocketRunWord, RunStateName } from '../ui';
import type { ExecutionSummary } from '../execution';

// What the board and the list say about each item's newest run, in the one run-state
// language, and the counts and filters built from it. Kept apart from the
// components so the mapping can be tested without mounting a board.

/** A card's newest run as the board shows it. */
export interface CardRun {
  state: RunStateName;
  /** The docket words the run carries, kept as the chip's tooltip. */
  docket?: DocketRunWord;
  /** Still going: it holds an agent or waits for one. A card with such a run offers no second run. */
  live: boolean;
  /** The run waits for an answer, so the card opens on that run's question. */
  asking: boolean;
  requestId?: string;
}

/** A project's default priority: what a new item gets, and so what a card need not repeat. */
export const DEFAULT_PRIORITY = 'medium';

const LIVE: ReadonlySet<RunStateName> = new Set(['queued', 'running', 'needs']);

/**
 * The state a run is in, from the server's lifecycle word. A finished run with a
 * patch nobody has reviewed is Ready for review; an item flagged for review with no
 * run loaded yet still reads that way. `undefined` for a word this build does not know.
 */
export function cardRun(
  summary: Pick<ExecutionSummary, 'request_id' | 'state' | 'cancellation_requested_at'> | undefined,
  needsReview: boolean,
): CardRun | undefined {
  if (!summary) {
    return needsReview ? { state: 'review', docket: 'Needs your review', live: false, asking: false } : undefined;
  }
  const make = (state: RunStateName, docket?: DocketRunWord): CardRun => ({
    state,
    docket,
    live: LIVE.has(state),
    asking: summary.state === 'waiting_decision',
    requestId: summary.request_id,
  });
  switch (summary.state) {
    case 'queued':
      return make('queued');
    case 'leased':
    case 'preparing':
    case 'running':
      return make('running', summary.cancellation_requested_at ? 'Cancellation requested' : undefined);
    case 'waiting_decision':
      return make('needs', 'Needs a person');
    case 'needs_operator':
      return make('needs');
    case 'succeeded':
      return needsReview ? make('review', 'Needs your review') : make('done');
    case 'failed':
    case 'lost':
      return make('failed');
    case 'cancelled':
      return make('cancelled');
    default:
      return undefined;
  }
}

/** What a filter on the board can ask for: one run state, or "waiting for you". */
export type RunFilter = 'waiting' | 'queued' | 'running' | 'needs' | 'review' | 'failed';

/** The run states a filter lets through. */
const MATCHES: Record<RunFilter, readonly RunStateName[]> = {
  waiting: ['needs', 'review'],
  queued: ['queued'],
  running: ['running'],
  needs: ['needs'],
  review: ['review'],
  failed: ['failed'],
};

/** Whether a filter can be set on this state: Done and Cancelled have no counter. */
export function filterOf(state: RunStateName): RunFilter | undefined {
  return state in MATCHES ? (state as RunFilter) : undefined;
}

/** An empty set of filters lets every card through; otherwise a card passes when any filter takes its state. */
export function passesFilters(run: CardRun | undefined, filters: ReadonlySet<RunFilter>): boolean {
  if (filters.size === 0) return true;
  if (!run) return false;
  for (const filter of filters) if (MATCHES[filter].includes(run.state)) return true;
  return false;
}

export type RunCounts = Record<RunStateName, number>;

export function countRuns(runs: Iterable<CardRun | undefined>): RunCounts {
  return countStates([...runs].map((run) => run?.state));
}

/** Runs counted by state, from any list of states: the one place a count over run states is made. */
export function countStates(states: Iterable<RunStateName | undefined>): RunCounts {
  const counts: RunCounts = { queued: 0, running: 0, needs: 0, review: 0, done: 0, failed: 0, cancelled: 0 };
  for (const state of states) if (state) counts[state] += 1;
  return counts;
}

/**
 * The three counts a person acts on, with one meaning each wherever they are shown (the board's
 * strip, Flow's summary, the sidebar's list): Needs you is a run that asks a person something,
 * Ready for review is a finished run whose work waits to be looked at, and Failed is a run that
 * ended badly. A run is in exactly one of them, never two.
 */
export interface RunTally {
  needsYou: number;
  review: number;
  failed: number;
}

export function runTally(counts: Pick<RunCounts, 'needs' | 'review' | 'failed'>): RunTally {
  return { needsYou: counts.needs, review: counts.review, failed: counts.failed };
}

/** The tally in the states' own words: "1 needs you, 2 ready for review"; empty when nothing waits. */
export function tallySentence(tally: RunTally): string {
  const parts: string[] = [];
  if (tally.needsYou > 0) parts.push(`${tally.needsYou} ${tally.needsYou === 1 ? 'needs' : 'need'} you`);
  if (tally.review > 0) parts.push(`${tally.review} ready for review`);
  if (tally.failed > 0) parts.push(`${tally.failed} failed`);
  return parts.join(', ');
}

/** How many runs of any state there are: the strip is hidden when a project has never run anything. */
export function totalRuns(counts: RunCounts): number {
  return Object.values(counts).reduce((n, c) => n + c, 0);
}

/** A card's priority is shown when it is not the default, or always when the person asked. `none` is never shown. */
export function showsPriority(priority: string, always: boolean): boolean {
  return priority !== 'none' && (always || priority !== DEFAULT_PRIORITY);
}

const ALWAYS_PRIORITY_KEY = 'tack_board_always_priority';

/** The remembered choice to show every card's priority; off when nothing is stored or storage is unavailable. */
export function getAlwaysPriority(): boolean {
  try {
    return localStorage.getItem(ALWAYS_PRIORITY_KEY) === '1';
  } catch {
    return false;
  }
}

export function setAlwaysPriority(on: boolean): void {
  try {
    localStorage.setItem(ALWAYS_PRIORITY_KEY, on ? '1' : '0');
  } catch {
    /* ignore — a convenience, not state */
  }
}

/** Each item's newest run, from the summaries the execution store holds. */
export function newestRunByItem(summaries: Iterable<ExecutionSummary>): Map<string, ExecutionSummary> {
  const newest = new Map<string, ExecutionSummary>();
  for (const summary of summaries) {
    const prior = newest.get(summary.item_id);
    if (!prior || summary.created_at > prior.created_at) newest.set(summary.item_id, summary);
  }
  return newest;
}
