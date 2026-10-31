import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import PlanPanel from './PlanPanel';
import { flush } from '../../test/flush';
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

// What the project, its roles, its profiles and GET /api/teams say, for the type and "who does it" controls.
const PROJECT = { id: 'p1', vocabulary: {} };
const ROLES = [
  { id: 'r1', name: 'Backend' },
  { id: 'r2', name: 'Frontend' },
];
const PROFILES = {
  protocol_version: 1,
  data: [
    { agent_profile_id: 'ap_research', name: 'Researcher', tool_policy: { docket: { team: 'saas-research' } } },
    { agent_profile_id: 'ap_dev', name: 'Implementer', tool_policy: { docket: { team: 'my-saas-dev' } } },
    { agent_profile_id: 'ap_solo', name: 'Writer', tool_policy: { tools: { 'claude-code': [] } } },
  ],
};
const TEAMS = {
  teams: [
    {
      name: 'my-saas-dev',
      harness: 'docket',
      reported: true,
      profiles: [{ id: 'ap_dev', name: 'Implementer' }],
      report: {
        name: 'my-saas-dev',
        profile: 'Implementer',
        members: [
          { role: 'Implementer', model: 'm1' },
          { role: 'Reviewer', model: 'm2' },
        ],
        steps: [{ name: 'Build', kind: 'command' }],
      },
    },
  ],
};

/** Answers the panel's requests by URL; every POST to the accept endpoint is recorded. */
function routeFetch(artifact: unknown, created: unknown, posts: Array<{ url: string; body: any }>) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const json = (body: unknown, status = 200) =>
      Promise.resolve(new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } }));
    if (init?.method === 'POST') {
      posts.push({ url, body: JSON.parse(String(init.body)) });
      return json(created, 201);
    }
    if (url.includes('/agent-profiles')) return json(PROFILES);
    if (url.includes('/teams')) return json(TEAMS);
    // Strict about the project's id, as the real routes are: a request for another project finds nothing.
    if (url.endsWith('/projects/p1/roles')) return json(ROLES);
    if (url.endsWith('/projects/p1')) return json(PROJECT);
    if (url.includes('/projects/')) return json({ error: { message: 'not found' } }, 404);
    // `GET /items/{id}` answers { item, roles, dependencies }, the project id inside the item.
    if (url.includes('/items/')) return json({ item: { id: 'item_1', project_id: 'p1' }, roles: [], dependencies: [] });
    return json(artifact);
  });
}

function mount(props: Record<string, unknown>) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <PlanPanel {...(props as any)} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

async function settle() {
  await flush();
  await flush();
  await flush();
}

function clickButton(container: HTMLElement, text: string) {
  Array.from(container.querySelectorAll('button')).find((b) => b.textContent === text)!.click();
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('PlanPanel', () => {
  it('only_selected_subtasks_are_created', async () => {
    const posts: Array<{ url: string; body: any }> = [];
    routeFetch(PLAN, { created: [{ id: 'n1', title: 'Add the settings table' }, { id: 'n3', title: 'Settings page in the frontend' }] }, posts);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();

    expect(container.textContent).toContain('The Planner proposes 3 items');
    const boxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"][aria-label^="Create"]');
    expect(boxes.length).toBe(3);
    boxes[1].click();
    clickButton(container, 'Create selected');
    await settle();

    expect(posts.length).toBe(1);
    expect(posts[0].url).toContain('/items/item_1/subtasks-from-plan');
    expect(posts[0].body.artifact_id).toBe('plan_1');
    expect(posts[0].body.subtasks.map((s: any) => s.title)).toEqual(['Add the settings table', 'Settings page in the frontend']);
    expect(posts[0].body.subtasks[0].acceptance).toEqual(PLAN.subtasks[0].acceptance);
    expect(container.textContent).toContain('Created 2 items');
    const links = Array.from(container.querySelectorAll('a')).map((a) => a.getAttribute('href'));
    expect(links).toEqual(['?item=n1', '?item=n3']);
  });

  it('the_title_names_the_team_when_the_run_has_one', async () => {
    routeFetch(PLAN, {}, []);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1', team: 'saas-research' });
    await settle();
    expect(container.textContent).toContain('The team saas-research proposes 3 items');
  });

  it('the_title_for_a_file_says_this_file_proposes', async () => {
    routeFetch(PLAN, {}, []);
    const container = mount({ itemId: 'item_1', file: [{ title: 'Only one', description: '', acceptance: [], depends_on: [] }] });
    await settle();
    expect(container.textContent).toContain('This file proposes 1 item');
    expect(container.textContent).not.toContain('Planner');
  });

  it('a_role_the_project_has_is_not_marked_as_missing', async () => {
    const named = { v: '1', subtasks: [{ title: 'Settings', description: '', acceptance: [], depends_on: [], role: 'Backend' }, { title: 'Docs', description: '', acceptance: [], depends_on: [], role: 'Writer' }] };
    routeFetch(named, { created: [] }, []);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();

    const roles = Array.from(container.querySelectorAll<HTMLSelectElement>('select[aria-label="Role"]'));
    const labels = (select: HTMLSelectElement) => Array.from(select.options).map((o) => o.textContent);
    expect(labels(roles[0])).toEqual(['No role', 'Backend', 'Frontend']);
    expect(roles[0].value).toBe('Backend');
    // A role the project does not have is still shown, and said not to be there.
    expect(labels(roles[1])).toContain('Writer (not in this project)');
  });

  it('editing_a_rows_type_role_and_who_changes_the_request', async () => {
    const posts: Array<{ url: string; body: any }> = [];
    routeFetch(PLAN, { created: [{ id: 'n1', title: 'Add the settings table' }] }, posts);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();

    const type = container.querySelector<HTMLSelectElement>('select[aria-label="Type"]')!;
    type.value = 'bug';
    type.dispatchEvent(new Event('change'));
    const role = container.querySelector<HTMLSelectElement>('select[aria-label="Role"]')!;
    role.value = 'Backend';
    role.dispatchEvent(new Event('change'));
    const who = container.querySelector<HTMLSelectElement>('select[aria-label="Who does it"]')!;
    who.value = 'ap_dev';
    who.dispatchEvent(new Event('change'));
    await settle();

    clickButton(container, 'Create selected');
    await settle();
    const sent = posts[0].body.subtasks[0];
    expect(sent.type).toBe('bug');
    expect(sent.role).toBe('Backend');
    expect(sent.team).toBe('my-saas-dev');
  });

  it('unticking_a_parent_unticks_its_children', async () => {
    const nested = {
      v: '1',
      subtasks: [
        { title: 'Parent', description: '', acceptance: [], depends_on: [], estimate: 'L' },
        { title: 'Child', description: '', acceptance: [], depends_on: [], parent: 0 },
      ],
    };
    routeFetch(nested, { created: [{ id: 'n1', title: 'Parent' }] }, []);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();

    const parentBox = container.querySelector<HTMLInputElement>('input[aria-label="Create Parent"]')!;
    const childBox = container.querySelector<HTMLInputElement>('input[aria-label="Create Child"]')!;
    expect(childBox.checked).toBe(true);
    parentBox.click();
    await settle();
    expect(childBox.checked).toBe(false);
    expect(childBox.disabled).toBe(true);
  });

  it('after_creating_each_unknown_part_is_named_on_its_entry', async () => {
    const posts: Array<{ url: string; body: any }> = [];
    routeFetch(
      PLAN,
      {
        created: [
          { id: 'n1', title: 'Add the settings table', unknown_role: 'QA' },
          { id: 'n2', title: 'Expose settings over the API', unknown_type: 'spike' },
          { id: 'n3', title: 'Settings page in the frontend', unresolved_team: 'saas-research' },
        ],
      },
      posts,
    );
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();
    clickButton(container, 'Create selected');
    await settle();

    expect(container.textContent).toContain('Add the settings table: the project has no role "QA", so it has none.');
    expect(container.textContent).toContain('Expose settings over the API: the project has no type "spike", so it took the parent\'s type.');
    expect(container.textContent).toContain('Settings page in the frontend: no single profile names the team "saas-research", so nobody is set to do it.');
  });
});
