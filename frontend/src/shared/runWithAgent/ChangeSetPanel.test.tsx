import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ChangeSetPanel from './ChangeSetPanel';
import type { ChangeSet, Operation } from '../api/changeSets';
import { refusalReasons } from '../api/changeSets';
import { flush } from '../../test/flush';

const disposers: Array<() => void> = [];

const U1 = '11111111-1111-4111-8111-111111111111';
const U2 = '22222222-2222-4222-8222-222222222222';

const OPS: Operation[] = [
  { op: 'create', title: 'Set up billing', description: 'Plans and invoices', acceptance: ['An invoice renders'], depends_on: [], role: 'Backend', team: null, type: 'task', parent: U1 },
  { op: 'create', title: 'Webhook for payments', description: null, acceptance: [], depends_on: [0], parent: 0, team: 'my-saas-dev' },
  { op: 'who_does_it', item: U1, role: 'Backend', team: 'my-saas-dev' },
  { op: 'priority', item: U1, priority: 'high' },
  { op: 'move', item: U2, status: 'Ready' },
  { op: 'depend', blocked: U2, blocker: 0 },
  { op: 'comment', item: U1, text: 'Billing comes first' },
  { op: 'due', item: U2, due: '2026-12-01T00:00:00Z' },
];

const setOf = (over: Partial<ChangeSet> = {}, states: Array<'waiting' | 'applied' | 'rejected'> = []): ChangeSet => ({
  id: 'cs1',
  project: 'p1',
  created_at: new Date(Date.now() - 5 * 60_000).toISOString(),
  ops: OPS.map((operation, i) => ({ operation, state: states[i] ?? 'waiting' })),
  origin: 'mcp',
  principal: 'agent:c1',
  principal_name: 'Planner credential',
  reason: 'Billing blocks the launch, so it goes first',
  status: 'waiting',
  ...over,
});

const PROJECT = { id: 'p1', vocabulary: {}, workflow: { statuses: [{ name: 'Backlog', order: 0 }, { name: 'Ready', order: 1 }, { name: 'Done', order: 2 }] } };
const ROLES = [{ id: 'r1', name: 'Backend' }, { id: 'r2', name: 'Frontend' }];
const PROFILES = { protocol_version: 1, data: [{ agent_profile_id: 'ap_dev', name: 'Implementer', tool_policy: { docket: { team: 'my-saas-dev' } } }] };

interface Call {
  method: string;
  url: string;
  body: any;
}

/** Answers the panel's reads by URL; records every write and answers it with `reply`. */
function routeFetch(calls: Call[], reply: (call: Call) => Response) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = init?.method ?? 'GET';
    const json = (body: unknown, status = 200) =>
      Promise.resolve(new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } }));
    if (method !== 'GET') {
      const call = { method, url, body: init?.body ? JSON.parse(String(init.body)) : null };
      calls.push(call);
      return Promise.resolve(reply(call));
    }
    if (url.includes('/agent-profiles')) return json(PROFILES);
    if (url.includes('/teams')) return json({ teams: [] });
    if (url.endsWith('/projects/p1/roles')) return json(ROLES);
    if (url.endsWith('/projects/p1')) return json(PROJECT);
    if (url.includes(`/items/${U1}`)) return json({ item: { id: U1, title: 'Payments epic', project_id: 'p1' }, roles: [], dependencies: [] });
    if (url.includes(`/items/${U2}`)) return json({ item: { id: U2, title: 'Invoice page', project_id: 'p1' }, roles: [], dependencies: [] });
    return json({}, 404);
  });
}

const jsonResponse = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

function mount(changeSet: ChangeSet, onChange?: (c: ChangeSet) => void) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ChangeSetPanel changeSet={changeSet} onChange={onChange} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

async function settle() {
  for (let i = 0; i < 4; i += 1) await flush();
}

const button = (c: HTMLElement, text: string) => Array.from(c.querySelectorAll('button')).find((b) => b.textContent?.trim() === text)!;
const byLabel = <T extends HTMLElement>(c: HTMLElement, label: string) => c.querySelector<T>(`[aria-label="${label}"]`)!;

function type(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('ChangeSetPanel', () => {
  it('shows_the_creates_as_a_tree_and_every_other_kind_as_a_list_with_reason_origin_and_proposer', async () => {
    routeFetch([], () => jsonResponse({}));
    const c = mount(setOf());
    await settle();

    expect(c.querySelector('h4')!.textContent).toBe('Planner credential proposes 8 changes');
    expect(c.querySelector('[data-testid="change-set-reason"]')!.textContent).toBe('Billing blocks the launch, so it goes first');
    expect(c.querySelector('[data-testid="change-set-origin"]')!.textContent).toContain('Through the MCP server');
    expect(c.querySelector('[data-testid="change-set-origin"]')!.textContent).toContain('Planner credential');
    expect(c.querySelector('[data-testid="change-set-status"]')!.textContent).toBe('Waiting for you');

    // The two creates are a tree: the second is nested under the first.
    expect(c.querySelectorAll('[data-testid="plan-row"]').length).toBe(2);
    expect(c.querySelector('ul[aria-label="Sub-items of Set up billing"] [data-testid="plan-row"]')).not.toBeNull();
    // The six other kinds are a list, each with its own tick and its item named by title.
    const kinds = Array.from(c.querySelectorAll('[data-testid="change-op"]')).map((e) => e.getAttribute('data-kind'));
    expect(kinds).toEqual(['who_does_it', 'priority', 'move', 'depend', 'comment', 'due']);
    expect(c.querySelector('input[aria-label="Set the priority of Payments epic to High"]')).not.toBeNull();
    expect(c.querySelector('input[aria-label="Invoice page waits on Set up billing"]')).not.toBeNull();
    expect(c.textContent).toContain('2 items · 1 sub-item · 1 dependency · 6 other changes');
    expect(button(c, 'Apply 8 changes').hasAttribute('disabled')).toBe(false);
  });

  it('applies_only_the_ticked_operations_after_saving_the_edits', async () => {
    const calls: Call[] = [];
    const seen: ChangeSet[] = [];
    routeFetch(calls, (call) => {
      if (call.method === 'PATCH') return jsonResponse(setOf());
      return jsonResponse(setOf({ status: 'partly', decided_by: 'operator:local', decided_at: new Date().toISOString() }, ['applied', 'applied', 'applied', 'waiting', 'applied', 'applied', 'applied', 'applied']));
    });
    const c = mount(setOf(), (next) => seen.push(next));
    await settle();

    type(byLabel<HTMLTextAreaElement>(c, 'Text of the comment on Payments epic'), 'Billing first, then the webhook');
    byLabel<HTMLInputElement>(c, 'Priority of Payments epic').value = 'low';
    byLabel<HTMLInputElement>(c, 'Priority of Payments epic').dispatchEvent(new Event('change', { bubbles: true }));
    type(c.querySelectorAll<HTMLInputElement>('input[aria-label="Title"]')[1], 'Payment webhooks');
    // Leave the priority change out.
    c.querySelector<HTMLInputElement>('input[aria-label="Set the priority of Payments epic to Low"]')!.click();
    await settle();
    expect(button(c, 'Apply 7 changes')).toBeDefined();
    button(c, 'Apply 7 changes').click();
    await settle();

    const patches = calls.filter((x) => x.method === 'PATCH');
    expect(patches.map((p) => p.url.split('/').slice(-3).join('/'))).toEqual(['cs1/operations/1', 'cs1/operations/3', 'cs1/operations/6']);
    expect(patches[0].body.operation).toMatchObject({ op: 'create', title: 'Payment webhooks', parent: 0, depends_on: [0] });
    expect(patches[2].body.operation).toEqual({ op: 'comment', item: U1, text: 'Billing first, then the webhook' });
    const applied = calls.filter((x) => x.method === 'POST');
    expect(applied.length).toBe(1);
    expect(applied[0].url).toContain('/change-sets/cs1/apply');
    expect(applied[0].body).toEqual({ operations: [0, 1, 2, 4, 5, 6, 7] });
    // What is left is shown as what it is, and the owner of the panel is told.
    expect(c.querySelector('[data-testid="change-set-status"]')!.textContent).toBe('Partly applied');
    expect(seen.at(-1)!.status).toBe('partly');
    expect(c.querySelectorAll('[data-testid="change-op-state"]').length).toBe(5);
    expect(button(c, 'Apply 1 change')).toBeDefined();
  });

  it('keeps_a_reference_the_tree_does_not_draw_and_clears_the_profile_when_a_team_is_chosen', async () => {
    const calls: Call[] = [];
    routeFetch(calls, () => jsonResponse(setOf()));
    const c = mount(setOf());
    await settle();

    const who = c.querySelectorAll<HTMLSelectElement>('select[aria-label="Who does it"]')[0];
    who.value = 'ap_dev';
    who.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    button(c, 'Save edits').click();
    await settle();
    const patch = calls.find((x) => x.method === 'PATCH')!;
    expect(patch.url).toContain('/operations/0');
    expect(patch.body.operation).toMatchObject({ op: 'create', title: 'Set up billing', parent: U1, team: 'my-saas-dev', profile_id: null });
  });

  it('lists_every_reason_when_the_server_refuses', async () => {
    const calls: Call[] = [];
    routeFetch(calls, () =>
      jsonResponse(
        {
          error: {
            status: 400,
            code: 'bad_request',
            retryable: false,
            request_id: 'r',
            details: {},
            message: 'The change set cannot be applied, 3 reasons:\noperation 1: it would make a loop\noperation 4: the status Ready is a full column (WIP 2)\noperation 2: the role Ghost does not exist',
          },
        },
        400,
      ),
    );
    const c = mount(setOf());
    await settle();
    button(c, 'Apply all').click();
    await settle();

    const box = c.querySelector('[data-testid="change-set-refusal"]')!;
    expect(box.getAttribute('role')).toBe('alert');
    expect(box.querySelector('p')!.textContent).toBe('Nothing was applied, for 3 reasons:');
    expect(Array.from(box.querySelectorAll('li')).map((l) => l.textContent)).toEqual([
      'operation 1: it would make a loop',
      'operation 4: the status Ready is a full column (WIP 2)',
      'operation 2: the role Ghost does not exist',
    ]);
    // The change set is as it was: still waiting, still all there.
    expect(c.querySelector('[data-testid="change-set-status"]')!.textContent).toBe('Waiting for you');
    expect(calls.at(-1)!.body).toEqual({});
  });

  it('reads_a_single_reason_and_a_plain_failure', () => {
    expect(refusalReasons('The change set cannot be applied: operation 0: nowhere to go')).toEqual(['operation 0: nowhere to go']);
    expect(refusalReasons('Boom')).toEqual(['Boom']);
  });

  it('rejects_the_waiting_operations_with_a_reason', async () => {
    const calls: Call[] = [];
    routeFetch(calls, () => jsonResponse(setOf({ status: 'rejected', rejection: 'Not before the launch', decided_at: new Date().toISOString(), decided_by: 'operator:local' }, OPS.map(() => 'rejected'))));
    const c = mount(setOf());
    await settle();

    button(c, 'Reject…').click();
    await settle();
    const confirm = c.querySelector<HTMLButtonElement>('[data-testid="change-set-reject-confirm"]')!;
    expect(confirm.disabled).toBe(true);
    type(byLabel<HTMLInputElement>(c, 'Why you turn these changes down'), 'Not before the launch');
    await settle();
    expect(confirm.disabled).toBe(false);
    confirm.click();
    await settle();

    expect(calls).toHaveLength(1);
    expect(calls[0].url).toContain('/change-sets/cs1/reject');
    expect(calls[0].body).toEqual({ reason: 'Not before the launch' });
    expect(c.querySelector('[data-testid="change-set-status"]')!.textContent).toBe('Turned down');
    expect(c.textContent).toContain('Not before the launch');
    // Nothing waits any more: the buttons that decide are off, and it says so.
    expect(c.querySelector<HTMLButtonElement>('[data-testid="change-set-apply-all"]')!.disabled).toBe(true);
    expect(c.textContent).toContain('Nothing here waits for you.');
  });

  it('applies_all_with_an_empty_request', async () => {
    const calls: Call[] = [];
    routeFetch(calls, () => jsonResponse(setOf({ status: 'applied' }, OPS.map(() => 'applied'))));
    const c = mount(setOf());
    await settle();
    // Untick one: "Apply all" still means everything that waits.
    c.querySelector<HTMLInputElement>('input[aria-label="Set the priority of Payments epic to High"]')!.click();
    await settle();
    button(c, 'Apply all').click();
    await settle();
    expect(calls).toHaveLength(1);
    expect(calls[0].body).toEqual({});
    expect(c.querySelector('[data-testid="change-set-status"]')!.textContent).toBe('Applied');
  });

  it('unticking_an_item_unticks_the_items_nested_under_it', async () => {
    routeFetch([], () => jsonResponse({}));
    const c = mount(setOf());
    await settle();
    c.querySelector<HTMLInputElement>('input[aria-label="Create Set up billing"]')!.click();
    await settle();
    expect(c.querySelector<HTMLInputElement>('input[aria-label="Create Payment webhooks"], input[aria-label="Create Webhook for payments"]')!.checked).toBe(false);
    expect(button(c, 'Apply 6 changes')).toBeDefined();
  });
});
