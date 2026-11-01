import { describe, it, expect } from 'vitest';
import { cardRun, countRuns, passesFilters, showsPriority, totalRuns, runTally, tallySentence, countStates, type RunFilter } from './cardRun';

const at = (state: string, cancelled: string | null = null) =>
  cardRun({ request_id: 'r', state, cancellation_requested_at: cancelled }, false);

describe('cardRun', () => {
  it('maps the lifecycle words onto the seven run states', () => {
    expect(
      ['queued', 'leased', 'preparing', 'running', 'waiting_decision', 'needs_operator', 'succeeded', 'failed', 'lost', 'cancelled'].map(
        (state) => at(state)?.state,
      ),
    ).toEqual(['queued', 'running', 'running', 'running', 'needs', 'needs', 'done', 'failed', 'failed', 'cancelled']);
  });

  it('says a run is live only while it holds an agent or waits for one', () => {
    expect(['queued', 'running', 'waiting_decision', 'succeeded', 'failed'].map((s) => at(s)?.live)).toEqual([true, true, true, false, false]);
  });

  it('keeps a cancellation request as the docket word, still running', () => {
    expect(at('running', '2026-01-01T00:00:00Z')).toMatchObject({ state: 'running', docket: 'Cancellation requested' });
  });

  it('reads a finished run with a patch to review as ready for review, and a flagged item with no run too', () => {
    expect(cardRun({ request_id: 'r', state: 'succeeded', cancellation_requested_at: null }, true)?.state).toBe('review');
    expect(cardRun(undefined, true)?.state).toBe('review');
    expect(cardRun(undefined, false)).toBeUndefined();
  });

  it('does not invent a state for a word it does not know', () => {
    expect(at('teleporting')).toBeUndefined();
  });
});

describe('counts and filters', () => {
  const runs = [at('running'), at('running'), at('waiting_decision'), cardRun(undefined, true), at('failed'), at('succeeded'), undefined];

  it('counts every state; Needs you, Ready for review and Failed are three separate counts', () => {
    const counts = countRuns(runs);
    expect(counts).toMatchObject({ running: 2, needs: 1, review: 1, failed: 1, done: 1, queued: 0 });
    expect(runTally(counts)).toEqual({ needsYou: 1, review: 1, failed: 1 });
    expect(tallySentence(runTally(counts))).toBe('1 needs you, 1 ready for review, 1 failed');
    expect(tallySentence(runTally(countStates([])))).toBe('');
    expect(tallySentence(runTally(countStates(['needs', 'needs', 'review'])))).toBe('2 need you, 1 ready for review');
    expect(totalRuns(counts)).toBe(6);
    expect(totalRuns(countRuns([undefined]))).toBe(0);
  });

  it('lets everything through with no filter and any card that a set filter takes otherwise', () => {
    const pass = (filters: RunFilter[]) => runs.map((r) => passesFilters(r, new Set(filters)));
    expect(pass([])).toEqual(runs.map(() => true));
    expect(pass(['running'])).toEqual([true, true, false, false, false, false, false]);
    expect(pass(['waiting'])).toEqual([false, false, true, true, false, false, false]);
    expect(pass(['failed', 'queued'])).toEqual([false, false, false, false, true, false, false]);
  });
});

describe('showsPriority', () => {
  it('hides the default, shows the rest, honours "always", and never shows none', () => {
    expect(showsPriority('medium', false)).toBe(false);
    expect(showsPriority('high', false)).toBe(true);
    expect(showsPriority('low', false)).toBe(true);
    expect(showsPriority('medium', true)).toBe(true);
    expect(showsPriority('none', true)).toBe(false);
  });
});
