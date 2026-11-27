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

const paths = () => routes.map((r) => r.path);

describe('routes', () => {
  it('has_the_five_work_views_under_one_layout_and_no_Calendar_or_Table', () => {
    const workRoutes = routes.find((r) => r.path === '/projects/:id')!.children as { path: string }[];
    expect(workRoutes.map((r) => r.path)).toEqual(['/board', '/orchestration', '/list', '/timeline', '/sprint']);
  });

  it('has_Needs_you_as_a_project_destination_and_neither_Factory_metrics_nor_Change_sets', () => {
    expect(paths()).toContain('/projects/:id/needs-you');
    expect(paths()).not.toContain('/projects/:id/factory');
    expect(paths()).not.toContain('/projects/:id/change-sets');
  });

  it.each(['calendar', 'factory', 'change-sets', 'table'])('answers_Page_not_found_to_the_old_address_%s', async (old) => {
    const history = createMemoryHistory();
    history.set({ value: `/projects/p1/${old}?item=i7` });
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(() => <MemoryRouter history={history}>{routes}</MemoryRouter>, container);
    disposers.push(() => { dispose(); container.remove(); });
    await flush();
    await flush();
    expect(container.textContent).toContain('Page not found');
    expect(history.get()).toBe(`/projects/p1/${old}?item=i7`);
  });
});
