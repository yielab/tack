import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import PlanPanel from './PlanPanel';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

// docs/contracts/plan-v1/plan.example.json
const PLAN = {
  v: '1',
  subtasks: [
    {
      title: 'Add the settings table',
      description: 'Create the migration and the repository functions for per-user settings.',
      acceptance: ['The migration applies to an empty database', 'A setting can be read back after it is written'],
      depends_on: [],
      estimate: 'S',
    },
    {
      title: 'Expose settings over the API',
      description: 'Add GET and PUT routes for the settings and regenerate the OpenAPI document.',
      acceptance: ['GET returns the stored value', 'PUT with an unknown key is rejected'],
      depends_on: [],
      estimate: 'M',
    },
    {
      title: 'Settings page in the frontend',
      description: 'Render the settings form against the new routes.',
      acceptance: ['Saving a value shows it after a reload'],
      depends_on: [0, 1],
      estimate: 'L',
    },
  ],
};

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('PlanPanel', () => {
  it('only_selected_subtasks_are_created', async () => {
    const posts: Array<{ url: string; body: any }> = [];
    vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      if (init?.method === 'POST') {
        posts.push({ url, body: JSON.parse(String(init.body)) });
        return Promise.resolve(
          new Response(JSON.stringify({ created: [{ id: 'n1', title: 'Add the settings table' }, { id: 'n3', title: 'Settings page in the frontend' }] }), {
            status: 201,
            headers: { 'content-type': 'application/json' },
          }),
        );
      }
      return Promise.resolve(new Response(JSON.stringify(PLAN)));
    });
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => <PlanPanel requestId="exec_1" attemptNumber={1} artifactId="plan_1" itemId="item_1" />,
      container,
    );
    disposers.push(() => {
      dispose();
      container.remove();
    });
    await flush();
    await flush();

    expect(container.textContent).toContain('The Planner proposes 3 subtasks');
    expect(container.textContent).toContain('L · after 1, 2');
    const boxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
    expect(boxes.length).toBe(3);
    boxes[1].click();
    const buttons = Array.from(container.querySelectorAll('button'));
    buttons.find((b) => b.textContent === 'Create selected')!.click();
    await flush();
    await flush();

    expect(posts.length).toBe(1);
    expect(posts[0].url).toContain('/items/item_1/subtasks-from-plan');
    expect(posts[0].body.artifact_id).toBe('plan_1');
    expect(posts[0].body.subtasks.map((s: any) => s.title)).toEqual([
      'Add the settings table',
      'Settings page in the frontend',
    ]);
    expect(posts[0].body.subtasks[0].acceptance).toEqual(PLAN.subtasks[0].acceptance);
    expect(container.textContent).toContain('Created 2 subtasks');
    const links = Array.from(container.querySelectorAll('a')).map((a) => a.getAttribute('href'));
    expect(links).toEqual(['?item=n1', '?item=n3']);
  });
});
