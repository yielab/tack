import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import type { Resource } from 'solid-js';
import { ProjectContext, type ProjectContextValue } from '../../../shared/state/projectContext';
import type { ItemTypeUse, Project } from '../../../shared/types';
import { flush } from '../../../test/flush';
import TypesPanel, { keyFor } from './TypesPanel';

const use = (key: string, name: string, level: 'group' | 'work', items: number, over: Partial<ItemTypeUse> = {}): ItemTypeUse => ({
  key,
  name,
  level,
  color: 'neutral',
  order: 0,
  default_work: false,
  items,
  ...over,
});

let catalog: ItemTypeUse[] = [];
let calls: Array<{ method: string; url: string; body: unknown }> = [];
let refused: string | null = null;
let projectRefetches = 0;

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

beforeEach(() => {
  catalog = [
    use('epic', 'Epic', 'group', 2, { color: 'accent' }),
    use('feature', 'Feature', 'work', 0),
    use('task', 'Task', 'work', 5, { default_work: true }),
  ];
  calls = [];
  refused = null;
  projectRefetches = 0;
  vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = init?.method ?? 'GET';
    const body = init?.body ? JSON.parse(String(init.body)) : undefined;
    if (!url.includes('/api/projects/p1/types')) return Promise.resolve(json({}));
    calls.push({ method, url, body });
    if (method !== 'GET' && refused) return Promise.resolve(json({ error: refused }, 400));
    const key = /types\/([^/?]+)/.exec(url)?.[1];
    if (method === 'POST') catalog.push(use(body.key, body.name, body.level, 0, { color: body.color }));
    if (method === 'PATCH') {
      const type = catalog.find((t) => t.key === key)!;
      if (body.default_work) catalog.forEach((t) => (t.default_work = t.key === key));
      if (body.order !== undefined) {
        catalog.splice(catalog.indexOf(type), 1);
        catalog.splice(body.order, 0, type);
      }
      Object.assign(type, { ...(body.name ? { name: body.name } : {}), ...(body.color ? { color: body.color } : {}), ...(body.level ? { level: body.level } : {}) });
      return Promise.resolve(json(type));
    }
    if (method === 'DELETE') {
      const into = /merge_into=([^&]+)/.exec(url)?.[1];
      const gone = catalog.find((t) => t.key === key)!;
      if (into) catalog.find((t) => t.key === into)!.items += gone.items;
      catalog = catalog.filter((t) => t.key !== key);
      return Promise.resolve(new Response(null, { status: 204 }));
    }
    return Promise.resolve(json(catalog));
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

async function mount() {
  const ctx: ProjectContextValue = {
    projectId: () => 'p1',
    project: (() => ({ id: 'p1', item_types: [] })) as unknown as Resource<Project>,
    workflow: () => undefined,
    vocabulary: () => ({}),
    refetch: () => {
      projectRefetches += 1;
    },
  };
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(
    () => (
      <ProjectContext.Provider value={ctx}>
        <MemoryRouter>
          <Route path="/" component={() => <TypesPanel />} />
        </MemoryRouter>
      </ProjectContext.Provider>
    ),
    container,
  );
  await flush();
  await flush();
  return container;
}

const row = (c: ParentNode, key: string) => c.querySelector<HTMLElement>(`[data-testid="type-row"][data-key="${key}"]`)!;
const keys = (c: ParentNode) => [...c.querySelectorAll('[data-testid="type-row"]')].map((r) => r.getAttribute('data-key'));
const writes = () => calls.filter((c) => c.method !== 'GET');
const type = async (input: HTMLInputElement, value: string) => {
  input.value = value;
  input.dispatchEvent(new Event('input', { bubbles: true }));
  await flush();
};

describe('Settings → Types', () => {
  it('lists_one_row_per_type_with_its_kind_and_how_many_items_use_it', async () => {
    const c = await mount();
    expect(keys(c)).toEqual(['epic', 'feature', 'task']);
    const epic = row(c, 'epic');
    expect(epic.querySelector<HTMLSelectElement>('select[aria-label="Epic holds work or is work"]')!.selectedOptions[0].textContent).toBe('Holds work');
    expect(epic.querySelector('[data-testid="type-items"]')!.textContent).toBe('2 items');
    expect(row(c, 'task').querySelector<HTMLSelectElement>('select[aria-label="Task holds work or is work"]')!.selectedOptions[0].textContent).toBe('Work');
    expect(row(c, 'task').querySelector<HTMLInputElement>('[data-testid="type-default"]')!.checked).toBe(true);
  });

  it('fixes_the_kind_of_a_used_type_with_its_reason_and_leaves_an_unused_one_free', async () => {
    const c = await mount();
    const used = row(c, 'epic').querySelector<HTMLSelectElement>('select[aria-label="Epic holds work or is work"]')!;
    expect(used.disabled).toBe(true);
    const reason = row(c, 'epic').querySelector('[data-testid="type-kind-reason"]')!;
    expect(reason.textContent).toContain('Fixed while 2 items use it');
    expect(used.getAttribute('aria-describedby')).toBe(reason.id);
    const free = row(c, 'feature').querySelector<HTMLSelectElement>('select[aria-label="Feature holds work or is work"]')!;
    expect(free.disabled).toBe(false);
    expect(row(c, 'feature').querySelector('[data-testid="type-kind-reason"]')).toBeNull();
    free.value = 'group';
    free.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    await flush();
    expect(writes()).toEqual([expect.objectContaining({ method: 'PATCH', body: { level: 'group' } })]);
    expect(projectRefetches).toBeGreaterThan(0);
  });

  it('adds_a_type_with_a_key_made_from_its_name', async () => {
    const c = await mount();
    const form = c.querySelector<HTMLFormElement>('[data-testid="add-type"]')!;
    const add = [...form.querySelectorAll('button')].find((b) => b.textContent === 'Add type')!;
    expect(add.disabled).toBe(true);
    await type(form.querySelector<HTMLInputElement>('input[type="text"]')!, 'Work Order');
    expect(form.textContent).toContain('work-order');
    const kind = form.querySelectorAll('select')[0] as HTMLSelectElement;
    kind.value = 'group';
    kind.dispatchEvent(new Event('change', { bubbles: true }));
    expect(add.disabled).toBe(false);
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await flush();
    await flush();
    expect(writes()).toEqual([expect.objectContaining({ method: 'POST', body: { key: 'work-order', name: 'Work Order', level: 'group', color: 'neutral' } })]);
    expect(keys(c)).toEqual(['epic', 'feature', 'task', 'work-order']);
  });

  it('will_not_add_a_type_whose_key_exists_and_says_why', async () => {
    const c = await mount();
    const form = c.querySelector<HTMLFormElement>('[data-testid="add-type"]')!;
    await type(form.querySelector<HTMLInputElement>('input[type="text"]')!, 'task');
    const add = [...form.querySelectorAll('button')].find((b) => b.textContent === 'Add type')!;
    expect(add.disabled).toBe(true);
    expect(form.textContent).toContain('A type with this key already exists');
  });

  it('removes_a_used_type_by_moving_its_items_to_a_type_of_the_same_kind', async () => {
    catalog.find((t) => t.key === 'feature')!.items = 3;
    const c = await mount();
    (row(c, 'feature').querySelector('button[aria-label="Remove Feature"]') as HTMLButtonElement).click();
    await flush();
    const dialog = document.body.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Move its 3 items to…');
    const into = dialog.querySelector<HTMLSelectElement>('select')!;
    // Only a type of the same kind: the one other work type, never the epic.
    expect([...into.options].map((o) => o.value)).toEqual(['task']);
    expect(writes()).toEqual([]);
    [...dialog.querySelectorAll('button')].find((b) => b.textContent === 'Move and remove')!.click();
    await flush();
    await flush();
    expect(writes()).toEqual([expect.objectContaining({ method: 'DELETE', url: expect.stringContaining('/types/feature?merge_into=task') })]);
    expect(keys(c)).toEqual(['epic', 'task']);
    expect(row(c, 'task').querySelector('[data-testid="type-items"]')!.textContent).toBe('8 items');
    expect(document.body.querySelector('[role="dialog"]')).toBeNull();
  });

  it('removes_an_unused_type_after_saying_nothing_uses_it', async () => {
    const c = await mount();
    (row(c, 'feature').querySelector('button[aria-label="Remove Feature"]') as HTMLButtonElement).click();
    await flush();
    const dialog = document.body.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('No item uses Feature');
    expect(dialog.querySelector('select')).toBeNull();
    [...dialog.querySelectorAll('button')].find((b) => b.textContent === 'Remove')!.click();
    await flush();
    await flush();
    expect(writes()).toEqual([expect.objectContaining({ method: 'DELETE', url: expect.not.stringContaining('merge_into') })]);
  });

  it('keeps_the_default_type_and_offers_no_default_for_a_type_that_holds_work', async () => {
    const c = await mount();
    const remove = row(c, 'task').querySelector<HTMLButtonElement>('button[aria-label="Remove Task"]')!;
    expect(remove.disabled).toBe(true);
    expect(row(c, 'task').textContent).toContain('Task is the default type');
    expect(row(c, 'epic').querySelector<HTMLInputElement>('[data-testid="type-default"]')!.disabled).toBe(true);
    const feature = row(c, 'feature').querySelector<HTMLInputElement>('[data-testid="type-default"]')!;
    feature.checked = true;
    feature.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    await flush();
    expect(writes()).toEqual([expect.objectContaining({ method: 'PATCH', body: { default_work: true } })]);
    expect(row(c, 'feature').querySelector<HTMLInputElement>('[data-testid="type-default"]')!.checked).toBe(true);
    expect(row(c, 'task').querySelector<HTMLInputElement>('[data-testid="type-default"]')!.checked).toBe(false);
  });

  it('moves_a_type_with_the_arrow_keys_on_its_handle_and_says_where_it_went', async () => {
    const c = await mount();
    const handle = row(c, 'epic').querySelector<HTMLElement>('[data-testid="type-handle"]')!;
    handle.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
    await flush();
    await flush();
    expect(writes()).toEqual([expect.objectContaining({ method: 'PATCH', url: expect.stringContaining('/types/epic'), body: { order: 1 } })]);
    expect(keys(c)).toEqual(['feature', 'epic', 'task']);
    expect(c.querySelector('[data-testid="types-announce"]')!.textContent).toBe('Epic is now number 2 of 3');
    // The first type cannot go up: nothing is sent.
    calls = [];
    row(c, 'feature').querySelector<HTMLElement>('[data-testid="type-handle"]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true }));
    await flush();
    expect(writes()).toEqual([]);
  });

  it('puts_a_refused_change_back_to_what_the_server_holds', async () => {
    const c = await mount();
    refused = 'Cannot do that';
    const free = row(c, 'feature').querySelector<HTMLSelectElement>('select[aria-label="Feature holds work or is work"]')!;
    free.value = 'group';
    free.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    await flush();
    expect(free.value).toBe('work');
  });
});

describe('keyFor', () => {
  it('makes_a_lowercase_key_of_letters_digits_and_dashes', () => {
    expect(keyFor('Work Order')).toBe('work-order');
    expect(keyFor('  Défect #2 ')).toBe('d-fect-2');
    expect(keyFor('???')).toBe('');
    expect(keyFor('a'.repeat(60))).toHaveLength(40);
  });
});
