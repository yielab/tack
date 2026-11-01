import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, createMemoryHistory } from '@solidjs/router';
import { routes } from './routes';
import { flush } from '../test/flush';

const disposers: Array<() => void> = [];
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

describe('routes', () => {
  it('sends_the_old_Table_address_to_the_List_and_keeps_the_open_item', async () => {
    const workRoutes = routes.find((r) => r.path === '/projects/:id')!.children as { path: string }[];
    expect(workRoutes.map((r) => r.path)).toEqual(['/board', '/list', '/table', '/calendar', '/timeline', '/sprint']);

    // Only the redirecting child is mounted here, under a stand-in for the work layout.
    const redirect = (workRoutes.find((r) => r.path === '/table') as unknown as { component: () => unknown }).component;
    const history = createMemoryHistory();
    history.set({ value: '/projects/p1/table?item=i7' });
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <MemoryRouter history={history}>
          {[
            { path: '/projects/:id/table', component: redirect as never },
            { path: '/projects/:id/list', component: () => <p id="list">the list</p> },
          ]}
        </MemoryRouter>
      ),
      container,
    );
    disposers.push(() => { dispose(); container.remove(); });
    await flush();
    await flush();
    expect(history.get()).toBe('/projects/p1/list?item=i7');
    expect(container.querySelector('#list')).not.toBeNull();
  });
});
