import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { MemoryRouter, Route } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../state/projectContext';
import Sidebar from './Sidebar';
import { flush } from '../../test/flush';

const settle = async () => {
  for (let i = 0; i < 8; i += 1) await flush();
};

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
  it('counts the runs that wait for an answer and opens a row for each, on its Execution tab', async () => {
    const c = mount([
      run('req-2', 'item-b', 'waiting_decision', '2026-10-31T11:00:00Z'),
      run('req-1', 'item-a', 'waiting_decision', '2026-10-31T10:00:00Z'),
      run('req-0', 'item-c', 'running', '2026-10-31T09:00:00Z'),
    ]);
    await settle();
    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('2');
    c.querySelector<HTMLButtonElement>('#waiting-toggle')!.click();
    await flush();
    const links = [...c.querySelectorAll('[data-testid="waiting-group-ask"] a')];
    expect(links.map((a) => new URL(a.getAttribute('href')!, 'http://x').searchParams.get('run'))).toEqual(['req-2', 'req-1']);
    const href = new URL(links[0].getAttribute('href')!, 'http://x');
    expect(href.searchParams.get('item')).toBe('item-b');
    expect(href.searchParams.get('tab')).toBe('execution');
  });

  it('a run that has a newer run after it no longer counts', async () => {
    const c = mount([
      run('req-2', 'item-a', 'succeeded', '2026-10-31T11:00:00Z'),
      run('req-1', 'item-a', 'waiting_decision', '2026-10-31T10:00:00Z'),
    ]);
    await settle();
    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('0');
  });

  it('stays in the sidebar with a 0 when no run waits', async () => {
    const c = mount([run('req-1', 'item-a', 'running', '2026-10-31T10:00:00Z')]);
    await settle();
    expect(c.textContent).toContain('Waiting for you');
    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('0');
    expect(c.querySelector<HTMLButtonElement>('#waiting-toggle')!.title).toContain('Nothing is waiting for you');
  });
});
