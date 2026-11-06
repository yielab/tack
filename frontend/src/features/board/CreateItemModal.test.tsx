import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import CreateItemModal from './CreateItemModal';
import { flush } from '../../test/flush';

const disposers: Array<() => void> = [];

const PROFILES = [
  { agent_profile_id: 'profile-1', name: 'Reviewer', instructions: 'x', tool_policy: { agent: 'codex' }, limits: null, summary: 'Reads the change' },
  { agent_profile_id: 'profile-2', name: 'Writer', instructions: 'x', tool_policy: { agent: 'docket', docket: { team: 'saas-content' } }, limits: null },
];

/** Answers the dialog's requests and records the writes it makes, in order. */
function routeFetch(
  writes: Array<{ method: string; url: string; body: unknown }>,
  profiles: unknown[] = PROFILES,
  /** What `GET /projects/{id}/parents?type=` answers, by the type asked about. */
  parents: Record<string, unknown[]> = {},
  asked: string[] = [],
) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const json = (body: unknown, status = 200, headers: Record<string, string> = {}) =>
      Promise.resolve(new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json', ...headers } }));
    const method = init?.method ?? 'GET';
    if (method !== 'GET') writes.push({ method, url, body: JSON.parse(String(init?.body ?? 'null')) });
    if (method === 'POST' && url.endsWith('/items')) return json({ id: 'item-new', project_id: 'p1' }, 201);
    if (method === 'PATCH') return json({ id: 'item-new', project_id: 'p1' }, 200, { ETag: '"2"' });
    if (url.includes('/parents?')) {
      const type = new URL(url, 'http://x').searchParams.get('type') ?? '';
      asked.push(type);
      return json(parents[type] ?? []);
    }
    if (url.includes('/agent-profiles')) return json({ protocol_version: 1, data: profiles });
    if (url.includes('/runners')) return json({ protocol_version: 1, data: [] });
    if (url.includes('/teams')) return json({ teams: [] });
    return json({ id: 'item-new', project_id: 'p1' }, 200, { ETag: '"1"' });
  });
}

const CONSTRUCTION = [
  { key: 'epic', name: 'Building', level: 'group', color: 'accent', order: 0, default_work: false },
  { key: 'feature', name: 'Section', level: 'group', color: 'accent2', order: 1, default_work: false },
  { key: 'task', name: 'Work Order', level: 'work', color: 'neutral', order: 2, default_work: true },
  { key: 'activity', name: 'Activity', level: 'work', color: 'success', order: 3, default_work: false },
  { key: 'bug', name: 'Defect', level: 'work', color: 'danger', order: 4, default_work: false },
  { key: 'requirement', name: 'Specification', level: 'work', color: 'warning', order: 5, default_work: false },
] as const;

function mount(itemTypes: readonly unknown[] = [], parentId?: string) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const onSuccess = vi.fn();
  const dispose = render(
    () => <CreateItemModal isOpen onClose={() => {}} onSuccess={onSuccess} projectId="p1" itemTypes={itemTypes as never} parentId={parentId} />,
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return { onSuccess };
}

async function settle() {
  await flush();
  await flush();
  await flush();
}

async function fillTitleAndCreate(title: string) {
  const input = document.querySelector<HTMLInputElement>('input[placeholder="What needs to be done?"]')!;
  input.value = title;
  input.dispatchEvent(new Event('input', { bubbles: true }));
  await settle();
  const create = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Create Item')!;
  create.click();
  await settle();
}

const radio = (name: string) => {
  const label = [...document.querySelectorAll<HTMLLabelElement>('[data-testid="who-does-it"] label')].find((l) => l.textContent === name)!;
  return document.getElementById(label.htmlFor) as HTMLInputElement;
};

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('CreateItemModal: who does it', () => {
  it('an_item_created_with_a_choice_carries_the_profile_in_its_run_settings', async () => {
    const writes: Array<{ method: string; url: string; body: unknown }> = [];
    routeFetch(writes);
    const { onSuccess } = mount();
    await settle();
    radio('Writer').click();
    await settle();
    expect(document.querySelector('[data-testid="create-who-does-it"] summary')?.textContent).toContain('Writer');
    await fillTitleAndCreate('Write the launch post');

    expect(writes.map((w) => `${w.method} ${w.url.replace('/api', '')}`)).toEqual(['POST /projects/p1/items', 'PATCH /items/item-new']);
    expect(writes[1].body).toEqual({ run_settings: { agent_profile_id: 'profile-2' } });
    expect(onSuccess).toHaveBeenCalled();
  });

  it('nobody_yet_creates_the_item_with_no_run_settings', async () => {
    const writes: Array<{ method: string; url: string; body: unknown }> = [];
    routeFetch(writes);
    mount();
    await settle();
    radio('Writer').click();
    radio('Nobody yet').click();
    await settle();
    await fillTitleAndCreate('Write the launch post');

    expect(writes.map((w) => w.method)).toEqual(['POST']);
  });

  it('with_no_profile_to_choose_the_dialog_offers_no_choice', async () => {
    routeFetch([], []);
    mount();
    await settle();
    expect(document.querySelector('[data-testid="create-who-does-it"]')).toBeNull();
  });

  it('the_choice_says_each_task_can_use_a_different_agent', async () => {
    routeFetch([]);
    mount();
    await settle();
    expect(document.querySelector('[data-testid="who-does-it"]')?.textContent).toContain(
      'Each task can use a different agent; this choice is for this task.',
    );
  });
});

describe('CreateItemModal: type', () => {
  const typeButtons = () => [...document.querySelectorAll<HTMLButtonElement>('[aria-labelledby$="-type"] button')];

  it("lists_a_construction_projects_types_with_its_default_chosen", async () => {
    routeFetch([]);
    mount(CONSTRUCTION);
    await settle();
    expect(typeButtons().map((b) => b.textContent)).toEqual(['Building', 'Section', 'Work Order', 'Activity', 'Defect', 'Specification']);
    expect(typeButtons().filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.textContent)).toEqual(['Work Order']);
  });

  it("creates_the_item_with_the_key_of_the_type_chosen_and_none_when_none_was", async () => {
    const writes: Array<{ method: string; url: string; body: unknown }> = [];
    routeFetch(writes);
    mount(CONSTRUCTION);
    await settle();
    typeButtons().find((b) => b.textContent === 'Defect')!.click();
    await settle();
    await fillTitleAndCreate('Cracked slab');
    expect(writes[0].body).toMatchObject({ title: 'Cracked slab', item_type: 'bug' });

    const plain: Array<{ method: string; url: string; body: unknown }> = [];
    document.body.innerHTML = '';
    vi.restoreAllMocks();
    routeFetch(plain);
    mount(CONSTRUCTION);
    await settle();
    await fillTitleAndCreate('Pour the slab');
    expect((plain[0].body as { item_type?: string }).item_type).toBeUndefined();
  });
});

describe('CreateItemModal: types by level and the parent', () => {
  const typeButtons = () => [...document.querySelectorAll<HTMLButtonElement>('[aria-labelledby$="-type"] button')];
  const checkout = { id: 'epic-1', title: 'Checkout', type: 'epic' };
  const task = { id: 'task-1', title: 'Pay', type: 'task' };

  it('lists_the_types_under_holds_work_and_work', async () => {
    routeFetch([]);
    mount(CONSTRUCTION);
    await settle();
    const names = (id: string) => [...document.querySelectorAll(`[data-testid="create-types-${id}"] button`)].map((b) => b.textContent);
    expect(names('holds')).toEqual(['Building', 'Section']);
    expect(names('work')).toEqual(['Work Order', 'Activity', 'Defect', 'Specification']);
    expect(document.querySelector('[data-testid="create-types-holds"]')!.getAttribute('aria-label')).toBe('Holds work');
    expect(document.querySelector('[data-testid="create-types-work"]')!.getAttribute('aria-label')).toBe('Work');
  });

  it('a_catalog_with_no_type_that_holds_work_shows_no_headings', async () => {
    routeFetch([]);
    mount(CONSTRUCTION.filter((t) => t.level === 'work'));
    await settle();
    expect(document.querySelector('[aria-label="Holds work"]')).toBeNull();
    expect(typeButtons()).toHaveLength(4);
  });

  it('has_no_subtasks_section_and_always_has_a_parent_field', async () => {
    routeFetch([]);
    mount(CONSTRUCTION);
    await settle();
    expect(document.body.textContent).not.toContain('Subtasks');
    expect(document.querySelector('[data-testid="create-parent"]')!.textContent).toContain('No parent');
  });

  it('the_parent_a_lane_or_group_presets_is_named_and_sent_with_the_item', async () => {
    const writes: Array<{ method: string; url: string; body: unknown }> = [];
    routeFetch(writes, PROFILES, { task: [checkout, task] });
    mount(CONSTRUCTION, 'epic-1');
    await settle();
    expect(document.querySelector('[data-testid="create-parent"]')!.textContent).toContain('Checkout');
    await fillTitleAndCreate('Pay by card');
    expect(writes[0].body).toMatchObject({ title: 'Pay by card', parent_id: 'epic-1' });
  });

  it('the_picker_offers_only_the_parents_the_server_admits_for_the_chosen_type', async () => {
    const asked: string[] = [];
    routeFetch([], PROFILES, { task: [checkout, task], epic: [] }, asked);
    mount(CONSTRUCTION);
    await settle();
    [...document.querySelectorAll<HTMLButtonElement>('[data-testid="create-parent"] button')].find((b) => b.textContent === 'Choose parent')!.click();
    await settle();
    const options = () => [...document.querySelectorAll('[data-testid="parent-picker"] [role="option"]')].map((o) => o.textContent);
    expect(options()).toEqual(['No parentCurrent', 'EpicCheckout', 'TaskPay']);
    expect(asked).toContain('task');

    // A type that can go nowhere offers nothing but the top.
    typeButtons().find((b) => b.textContent === 'Building')!.click();
    await settle();
    expect(asked).toContain('epic');
    expect(options()).toEqual(['No parentCurrent']);
  });

  it('choosing_a_parent_sends_it_and_a_type_that_cannot_go_under_it_drops_it_with_a_word_about_why', async () => {
    const writes: Array<{ method: string; url: string; body: unknown }> = [];
    routeFetch(writes, PROFILES, { task: [checkout, task], epic: [] });
    mount(CONSTRUCTION);
    await settle();
    [...document.querySelectorAll<HTMLButtonElement>('[data-testid="create-parent"] button')].find((b) => b.textContent === 'Choose parent')!.click();
    await settle();
    [...document.querySelectorAll<HTMLElement>('[data-testid="parent-picker"] [role="option"]')].find((o) => o.textContent?.includes('Checkout'))!.click();
    await settle();
    expect(document.querySelector('[data-testid="create-parent"]')!.textContent).toContain('Checkout');

    typeButtons().find((b) => b.textContent === 'Building')!.click();
    await settle();
    expect(document.querySelector('[data-testid="create-parent"]')!.textContent).toContain('No parent');
    expect(document.querySelector('[data-testid="create-parent"] [role="status"]')?.textContent).toBe('Building cannot go under that parent, so it goes at the top.');
    await fillTitleAndCreate('Annex');
    expect((writes[0].body as { parent_id?: string | null }).parent_id).toBeNull();
  });
});
