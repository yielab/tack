import { describe, it, expect, vi, afterEach } from 'vitest';
import { createComponent, type Resource } from 'solid-js';
import { render } from 'solid-js/web';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import type { Project } from '../../shared/types';
import Dashboard from './Dashboard';
import { flush } from '../../test/flush';

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

/** Opens the Overview at `address`; the items are none and the metrics and autonomy reads fail, so each part is told apart by what it says. */
function open(address: string) {
  const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes('/metrics/factory') || url.includes('/autonomy')) {
      return new Response(JSON.stringify({ message: 'down' }), { status: 503 });
    }
    return new Response(JSON.stringify({ data: [], total: 0 }), { status: 200 });
  }) as typeof fetch);
  const project = { id: 'p1', name: 'P', workflow: { statuses: [] } } as unknown as Project;
  const ctx: ProjectContextValue = {
    projectId: () => 'p1',
    project: (() => project) as unknown as Resource<Project>,
    workflow: () => project.workflow,
    vocabulary: () => ({}),
    refetch: () => {},
  };
  const history = createMemoryHistory();
  history.set({ value: address });
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () =>
      createComponent(ExecutionStoreProvider, {
        get children() {
          return createComponent(ProjectContext.Provider, {
            value: ctx,
            get children() {
              return createComponent(MemoryRouter, {
                history,
                get children() {
                  return createComponent(Route, { path: '/projects/:id/overview', component: Dashboard });
                },
              });
            },
          });
        },
      }),
    container,
  );
  const pressed = () =>
    [...container.querySelectorAll('[data-testid="overview-parts"] button')]
      .filter((b) => b.getAttribute('aria-pressed') === 'true')
      .map((b) => b.textContent);
  const urls = () => fetchMock.mock.calls.map((call) => String(call[0]));
  return { container, dispose, history, pressed, urls };
}

describe('Overview parts', () => {
  it('opens on Progress, with the three parts in a switch under the title and no period', async () => {
    const { container, dispose, pressed } = open('/projects/p1/overview');
    await flush();
    expect(container.querySelector('h1')!.textContent).toBe('Overview');
    expect([...container.querySelectorAll('[data-testid="overview-parts"] button')].map((b) => b.textContent)).toEqual([
      'Progress',
      "Agents' work",
      'Autonomy by domain',
    ]);
    expect(pressed()).toEqual(['Progress']);
    expect(container.querySelector('select')).toBeNull();
    dispose();
  });

  it('opens the part the address names, and Progress for a name it does not know', async () => {
    const agents = open('/projects/p1/overview?part=agents');
    await vi.waitFor(() => expect(agents.container.textContent).toContain("Could not load the agents' work."));
    expect(agents.pressed()).toEqual(["Agents' work"]);
    expect(agents.container.querySelector('select')).not.toBeNull();
    agents.dispose();

    const autonomy = open('/projects/p1/overview?part=autonomy');
    await vi.waitFor(() => expect(autonomy.container.textContent).toContain('Could not load the autonomy settings.'));
    expect(autonomy.pressed()).toEqual(['Autonomy by domain']);
    autonomy.dispose();

    const unknown = open('/projects/p1/overview?part=nonsense');
    await flush();
    expect(unknown.pressed()).toEqual(['Progress']);
    unknown.dispose();
  });

  it('holds the part in the address, and leaves it out for Progress', async () => {
    const { container, dispose, history, pressed } = open('/projects/p1/overview');
    await flush();
    container.querySelector<HTMLButtonElement>('[data-testid="overview-part-agents"]')!.click();
    await flush();
    expect(history.get()).toContain('part=agents');
    expect(pressed()).toEqual(["Agents' work"]);

    container.querySelector<HTMLButtonElement>('[data-testid="overview-part-progress"]')!.click();
    await flush();
    expect(history.get()).not.toContain('part=');
    expect(pressed()).toEqual(['Progress']);
    dispose();
  });

  it('reads the agents\' work and the autonomy over the period chosen, from one select', async () => {
    const { container, dispose, urls } = open('/projects/p1/overview?part=agents');
    await flush();
    const select = container.querySelector('select')!;
    select.value = '7';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    await flush();
    expect(urls().some((url) => url.includes('/metrics/factory?since='))).toBe(true);

    container.querySelector<HTMLButtonElement>('[data-testid="overview-part-autonomy"]')!.click();
    await flush();
    await flush();
    expect(container.querySelector('select')!.value).toBe('7');
    expect(urls().filter((url) => url.includes('/metrics/factory?since=')).length).toBeGreaterThan(1);
    dispose();
  });
});
