import { describe, expect, it, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import RunLimitNote, { type RunLimitNoteProps } from './RunLimitNote';
import type { RunLimit } from '../execution/api';

vi.mock('./RunWithAgentModal', () => ({ default: () => null }));

const limit = (used: number | null): RunLimit => ({
  tokens: 1000,
  source: 'profile',
  enforced_by: 'docket',
  sentence: '1 000 tokens · from the profile Implementer',
  used,
});

function mount(props: RunLimitNoteProps) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <RunLimitNote {...props} />, container);
  return container;
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('RunLimitNote', () => {
  it('shows a running count in the normal colour below 80% and amber from it', () => {
    const below = mount({ limit: limit(799), running: true, stopped: false, testId: 't' });
    const used = below.querySelector('[data-testid="run-limit-used"]')!;
    expect(used.textContent).toBe('Used 799 of 1 000 tokens');
    expect(used.getAttribute('data-near-limit')).toBe('false');
    const near = mount({ limit: limit(800), running: true, stopped: false, testId: 't' });
    expect(near.querySelector('[data-testid="run-limit-used"]')!.getAttribute('data-near-limit')).toBe('true');
  });

  it('says how much a stopped run used and offers to run again with more', () => {
    const c = mount({ limit: limit(1000), running: false, stopped: true, testId: 't', itemId: 'i', itemTitle: 'Task', projectId: 'p' });
    expect(c.querySelector('[data-testid="run-limit-stopped"]')!.textContent).toBe('Stopped at its limit: 1 000 of 1 000');
    expect(c.textContent).toContain('Run again with more');
  });

  it('shows no count for a finished run', () => {
    const c = mount({ limit: limit(500), running: false, stopped: false, testId: 't' });
    expect(c.querySelector('[data-testid="run-limit-used"]')).toBeNull();
    expect(c.querySelector('[data-testid="run-limit-stopped"]')).toBeNull();
  });
});
