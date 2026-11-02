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
    clickButton(container, 'Create 2 items');
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

  it('names_every_field_of_an_entry_and_writes_the_description_in_a_text_area', async () => {
    routeFetch(PLAN, { created: [] }, []);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();
    const row = container.querySelector('[data-testid="plan-row"]')!;
    expect([...row.querySelectorAll('[data-field-label]')].map((l) => l.textContent)).toEqual([
      'Title',
      'Description',
      'Done when (one line each)',
      'Type',
      'Role',
      'Who does it',
    ]);
    // Each caption names its control, so the control's accessible name is the words on screen.
    for (const label of row.querySelectorAll<HTMLLabelElement>('label[data-field-label]')) {
      expect(label.control, label.textContent!).not.toBeNull();
    }
    const description = row.querySelector('[aria-label="Description"]')!;
    expect(description.tagName).toBe('TEXTAREA');
  });

  it('sends_the_done_when_lines_of_an_entry_that_has_sub_items_as_well_as_theirs', async () => {
    const posts: Array<{ url: string; body: any }> = [];
    const nested = {
      v: '1',
      subtasks: [
        { title: 'Sharing', description: '', acceptance: ['Both child items are done', 'Access can be revoked'], depends_on: [] },
        { title: 'Model', description: '', acceptance: ['A design note lists the roles'], depends_on: [], parent: 0 },
      ],
    };
    routeFetch(nested, { created: [] }, posts);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();
    // The whole of a long list is shown, not its first lines only.
    const first = container.querySelector<HTMLTextAreaElement>('textarea[aria-label^="Done when"]')!;
    expect(first.value).toBe('Both child items are done\nAccess can be revoked');
    clickButton(container, 'Create 2 items');
    await settle();
    const sent = posts[0].body.subtasks;
    expect(sent[0].acceptance).toEqual(['Both child items are done', 'Access can be revoked']);
    expect(sent[1].acceptance).toEqual(['A design note lists the roles']);
    expect(sent[1].parent).toBe(0);
  });

  it('says_what_becomes_of_a_type_the_project_does_not_have', async () => {
    const odd = { v: '1', subtasks: [{ title: 'Odd', description: '', acceptance: [], depends_on: [], type: 'saga' }, { title: 'Plain', description: '', acceptance: [], depends_on: [] }] };
    routeFetch(odd, { created: [] }, []);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();
    const rows = container.querySelectorAll('[data-testid="plan-row"]');
    const note = rows[0].querySelector('[data-testid="plan-type-note"]')!;
    expect(note.textContent).toBe('This project has no type "saga": the item will be created with its parent\'s type.');
    expect(rows[1].querySelector('[data-testid="plan-type-note"]')).toBeNull();
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

    clickButton(container, 'Create 3 items');
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
    clickButton(container, 'Create 3 items');
    await settle();

    expect(container.textContent).toContain('Add the settings table: the project has no role "QA", so it has none.');
    expect(container.textContent).toContain('Expose settings over the API: the project has no type "spike", so it took the parent\'s type.');
    expect(container.textContent).toContain('Settings page in the frontend: no single profile names the team "saas-research", so nobody is set to do it.');
  });

  it('a_sub_item_is_nested_under_its_parent_and_a_deeper_one_under_that', async () => {
    const deep = {
      v: '1',
      subtasks: [
        { title: 'Plan', description: '', acceptance: [], depends_on: [] },
        { title: 'Table', description: '', acceptance: [], depends_on: [], parent: 0 },
        { title: 'Page', description: '', acceptance: [], depends_on: [0] },
        { title: 'Migration', description: '', acceptance: [], depends_on: [], parent: 1 },
      ],
    };
    routeFetch(deep, {}, []);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();

    const outer = container.querySelector<HTMLUListElement>('ul[aria-label="Proposed items"]')!;
    // The top level holds Plan and Page; Table sits in a list inside Plan's item, Migration inside Table's.
    const top = Array.from(outer.children);
    expect(top.length).toBe(2);
    const plan = top[0];
    const planSub = plan.querySelector<HTMLUListElement>(':scope > ul')!;
    expect(planSub.getAttribute('aria-label')).toBe('Sub-items of Plan');
    expect(planSub.parentElement).toBe(plan);
    const tableSub = planSub.querySelector<HTMLUListElement>(':scope > li > ul')!;
    expect(tableSub.getAttribute('aria-label')).toBe('Sub-items of Table');
    expect(tableSub.querySelector('input[aria-label="Title"]')).not.toBeNull();
    expect(top[1].querySelector('ul[aria-label^="Sub-items"]')).toBeNull();
  });

  it('the_header_counts_items_sub_items_and_dependencies_singular_and_plural', async () => {
    routeFetch(PLAN, {}, []);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();
    // PLAN has three items and two dependencies (the third waits on both others).
    expect(container.textContent).toContain('3 items · 2 dependencies');
    expect(container.textContent).not.toContain('sub-item');

    const one = mount({ itemId: 'item_1', file: [{ title: 'Only', description: '', acceptance: [], depends_on: [] }] });
    await settle();
    expect(one.textContent).toContain('1 item');
    expect(one.textContent).not.toContain('items ·');
    expect(one.textContent).not.toContain('dependenc');

    const many = mount({
      itemId: 'item_1',
      file: [
        { title: 'A', description: '', acceptance: [], depends_on: [] },
        { title: 'B', description: '', acceptance: [], depends_on: [], parent: 0 },
        { title: 'C', description: '', acceptance: [], depends_on: [], parent: 0 },
        { title: 'D', description: '', acceptance: [], depends_on: [0, 1] },
      ],
    });
    await settle();
    expect(many.textContent).toContain('4 items · 2 sub-items · 2 dependencies');
  });

  it('the_header_names_one_sub_item_and_one_dependency_in_the_singular', async () => {
    const nested = {
      v: '1',
      subtasks: [
        { title: 'Plan', description: '', acceptance: [], depends_on: [] },
        { title: 'Table', description: '', acceptance: [], depends_on: [], parent: 0 },
        { title: 'Page', description: '', acceptance: [], depends_on: [0] },
      ],
    };
    routeFetch(nested, {}, []);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();
    expect(container.textContent).toContain('3 items · 1 sub-item · 1 dependency');
  });

  it('select_none_unticks_every_item_and_select_all_ticks_them_back', async () => {
    routeFetch(PLAN, { created: [] }, []);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();
    const boxes = () => Array.from(container.querySelectorAll<HTMLInputElement>('input[type="checkbox"][aria-label^="Create"]'));

    clickButton(container, 'Select none');
    await settle();
    expect(boxes().map((b) => b.checked)).toEqual([false, false, false]);
    expect(Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Select none')!.disabled).toBe(true);
    expect(Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Create 0 items')!.disabled).toBe(true);

    clickButton(container, 'Select all');
    await settle();
    expect(boxes().map((b) => b.checked)).toEqual([true, true, true]);
    expect(Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Select all')!.disabled).toBe(true);
  });

  it('the_create_button_names_the_number_ticked', async () => {
    routeFetch(PLAN, { created: [] }, []);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();
    const boxes = () => Array.from(container.querySelectorAll<HTMLInputElement>('input[type="checkbox"][aria-label^="Create"]'));
    // The create button is the only button whose text starts with a count.
    const labelOf = () => Array.from(container.querySelectorAll('button')).map((b) => b.textContent).find((t) => /^Create \d+ /.test(t ?? ''));

    expect(labelOf()).toBe('Create 3 items');
    expect(container.querySelector('[data-testid="plan-create"]')?.textContent).toBe('Create 3 items');
    boxes()[0].click();
    await settle();
    expect(labelOf()).toBe('Create 2 items');
    boxes()[1].click();
    await settle();
    expect(labelOf()).toBe('Create 1 item');
  });

  it('waits_on_shows_titles_as_chips_and_a_menu_adds_one_back', async () => {
    const posts: Array<{ url: string; body: any }> = [];
    routeFetch(PLAN, { created: [] }, posts);
    const container = mount({ requestId: 'exec_1', attemptNumber: 1, artifactId: 'plan_1', itemId: 'item_1' });
    await settle();

    const third = container.querySelectorAll<HTMLElement>('[data-testid="plan-row"]')[2];
    const chips = () => Array.from(third.querySelectorAll('fieldset > ul > li > span')).map((s) => s.textContent);
    expect(chips()).toEqual(['Add the settings table', 'Expose settings over the API']);

    third.querySelector<HTMLButtonElement>('button[aria-label="Stop waiting on Expose settings over the API"]')!.click();
    await settle();
    expect(chips()).toEqual(['Add the settings table']);

    const menu = third.querySelector<HTMLSelectElement>('select[aria-label="Add a wait for Settings page in the frontend"]')!;
    expect(Array.from(menu.options).map((o) => o.textContent)).toEqual(['Add an item…', 'Expose settings over the API']);
    menu.value = '1';
    menu.dispatchEvent(new Event('change'));
    await settle();
    expect(chips()).toEqual(['Add the settings table', 'Expose settings over the API']);
    expect(menu.value).toBe('');

    clickButton(container, 'Create 3 items');
    await settle();
    expect(posts[0].body.subtasks[2].depends_on).toEqual([0, 1]);
  });
});
