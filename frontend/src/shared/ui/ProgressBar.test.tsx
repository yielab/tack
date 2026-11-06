import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Progress } from '../types';
import ProgressBar, { needsYouText, progressText } from './ProgressBar';

const progress = (p: Partial<Progress> = {}): Progress => ({
  total: 7, done: 3, open_runs: 0, needs_you: 0, estimate_total: null, estimate_done: null, starts: null, ends: null, teams: [], ...p,
});

const disposers: Array<() => void> = [];
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});
function mount(p: Progress | null | undefined, pointsWord?: string) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  disposers.push(render(() => <ProgressBar progress={p} pointsWord={pointsWord} />, container));
  return container;
}

describe('ProgressBar', () => {
  it('says_how_many_items_are_done_when_the_work_has_no_estimates', () => {
    const c = mount(progress());
    expect(c.querySelector('[data-testid="progress-text"]')!.textContent).toBe('3 of 7 done');
    const bar = c.querySelector('[role="progressbar"]')!;
    expect(bar.getAttribute('aria-valuenow')).toBe('43');
    expect(bar.getAttribute('aria-label')).toBe('3 of 7 done');
  });

  it('counts_points_when_the_work_has_estimates_in_the_projects_own_word', () => {
    const p = progress({ estimate_total: 20, estimate_done: 12 });
    expect(progressText(p)).toBe('12 of 20 points');
    const c = mount(p, 'story points');
    expect(c.querySelector('[data-testid="progress-text"]')!.textContent).toBe('12 of 20 story points');
    expect(c.querySelector('[role="progressbar"]')!.getAttribute('aria-valuenow')).toBe('60');
  });

  it('says_how_many_need_a_person_only_when_some_do', () => {
    expect(needsYouText(progress())).toBeNull();
    expect(needsYouText(progress({ needs_you: 1 }))).toBe('1 needs you');
    expect(needsYouText(progress({ needs_you: 3 }))).toBe('3 need you');
    expect(mount(progress()).querySelector('[data-testid="progress-needs"]')).toBeNull();
    expect(mount(progress({ needs_you: 2 })).querySelector('[data-testid="progress-needs"]')!.textContent).toBe('2 need you');
  });

  it('draws_nothing_without_work_under_the_group', () => {
    expect(mount(progress({ total: 0, done: 0 })).querySelector('[data-testid="progress"]')).toBeNull();
    expect(mount(null).querySelector('[data-testid="progress"]')).toBeNull();
    expect(mount(undefined).querySelector('[data-testid="progress"]')).toBeNull();
  });
});
