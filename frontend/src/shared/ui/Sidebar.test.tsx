import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { MemoryRouter, Route } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../state/projectContext';
import Sidebar from './Sidebar';
import { flush } from '../../test/flush';

const projectValue: ProjectContextValue = {
  projectId: () => undefined,
  project: (() => null) as unknown as Resource<null>,
  workflow: () => undefined,
  vocabulary: () => ({}),
  refetch: () => {},
} as unknown as ProjectContextValue;

const disposers: Array<() => void> = [];

const run = (id: string, item: string, state: string, created: string) => ({
  request_id: id, item_id: item, state, cancellation_requested_at: null, created_at: created,
});

function mount(executions: unknown[]) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    const body = url.includes('/executions') ? { protocol_version: 1, data: executions } : url.includes('/projects') ? [] : {};
    return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
  }) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={projectValue}>
        <MemoryRouter>
          <Route path="/" component={Sidebar} />
        </MemoryRouter>
      </ProjectContext.Provider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

describe('Sidebar: Waiting for you', () => {
  it('counts the runs that wait for an answer and links to the first, on its Execution tab', async () => {
    const c = mount([
      run('req-2', 'item-b', 'waiting_decision', '2026-10-31T11:00:00Z'),
      run('req-1', 'item-a', 'waiting_decision', '2026-10-31T10:00:00Z'),
      run('req-0', 'item-c', 'running', '2026-10-31T09:00:00Z'),
    ]);
    await flush();
    await flush();
    const link = [...c.querySelectorAll('a')].find((a) => a.textContent?.includes('Waiting for you'))!;
    expect(link).toBeTruthy();
    expect(link.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('2');
    const href = new URL(link.getAttribute('href')!, 'http://x');
    expect(href.searchParams.get('item')).toBe('item-b');
    expect(href.searchParams.get('tab')).toBe('execution');
    expect(href.searchParams.get('run')).toBe('req-2');
  });

  it('a run that has a newer run after it no longer counts', async () => {
    const c = mount([
      run('req-2', 'item-a', 'succeeded', '2026-10-31T11:00:00Z'),
      run('req-1', 'item-a', 'waiting_decision', '2026-10-31T10:00:00Z'),
    ]);
    await flush();
    await flush();
    expect(c.textContent).not.toContain('Waiting for you');
  });

  it('shows no row when no run waits', async () => {
    const c = mount([run('req-1', 'item-a', 'running', '2026-10-31T10:00:00Z')]);
    await flush();
    await flush();
    expect(c.textContent).not.toContain('Waiting for you');
    expect(c.querySelector('[data-testid="waiting-count"]')).toBeNull();
  });
});
