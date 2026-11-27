import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { createSignal, Show, type Resource } from 'solid-js';
import { MemoryRouter, Route } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import ItemBody from './ItemBody';
import { useItemEditor } from './useItemEditor';
import { flush } from '../../test/flush';

const ITEM = {
  id: 'item-1',
  project_id: 'p1',
  title: 'My item',
  item_type: 'task',
  status: 'todo',
  priority: 'high',
  assignee: 'Ana',
  tags: [],
  estimate_unit: 'story_points',
  sort_order: 0,
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
};

const TYPES = [
  { key: 'epic', name: 'Epic', level: 'group', color: 'accent', order: 0, default_work: false },
  { key: 'task', name: 'Task', level: 'work', color: 'neutral', order: 1, default_work: true },
];

const projectValue: ProjectContextValue = {
  projectId: () => 'p1',
  project: (() => ({ id: 'p1', item_types: TYPES })) as unknown as Resource<null>,
  workflow: () => ({ workflow_type: 'kanban', statuses: [{ name: 'todo', category: 'todo', order: 0 }] }),
  vocabulary: () => ({}),
  refetch: () => {},
};

const PROFILE = { agent_profile_id: 'prof-1', name: 'Code agent', instructions: 'x', tool_policy: { docket: { team: 'saas-code' } }, limits: null };
const RUN = { request_id: 'req-1', item_id: ITEM.id, state: 'running', cancellation_requested_at: null, created_at: '2026-10-08T12:00:00Z' };
const ATTEMPT = {
  attempt_id: 'att-1', request_id: 'req-1', attempt_number: 1, runner_id: 'r1', fencing_token: 1, state: 'running',
  lease_issued_at: '2026-10-08T12:00:00Z', lease_expires_at: '2026-10-08T12:05:00Z', last_heartbeat_at: null,
  event_checkpoint: null, completion_id: null, workspace_id: null, base_revision: null, actual_execution: null,
  terminal_reason: null, usage: null, counted_tokens: null, started_at: '2026-10-08T12:00:05Z', ended_at: null,
  created_at: '2026-10-08T12:00:00Z', updated_at: '2026-10-08T12:02:00Z', model_provenance: null, pull_request: null, review: null,
};

let runs: unknown[] = [];
/** What `GET /projects/p1/items?under=` answers: the work under the group. */
let underItems: unknown[] = [];
/** What `GET /projects/p1/parents` answers. */
let parentOptions: unknown[] = [];
/** The writes the screen made, in order. */
let writes: Array<{ method: string; url: string; body: unknown }> = [];
/** The project's items as the list endpoint answers them. */
let projectItems: unknown[] = [];
let autoStart = false;
/** Why auto-start has not started the item, as the server answers it; null lists nothing. */
let oneRunner = false;
/** What `GET /projects/p1/folder/status` answers; null is a project with no folder (404). */
let folderStatus: unknown = null;
let waitingReason: { code: string; text: string } | null = null;

function jsonOf(url: string): unknown {
  if (url.endsWith(`/api/items/${ITEM.id}`)) return { item: ITEM, roles: [], dependencies: [] };
  if (url.includes('/parents?')) return parentOptions;
  if (/\/projects\/p1\/items\?.*under=/.test(url)) return { data: underItems, total: underItems.length, page: 1, per_page: 200 };
  if (/\/projects\/p1\/items\?/.test(url)) return { data: projectItems, total: projectItems.length, page: 1, per_page: 200 };
  if (url.includes('/folder/status')) return folderStatus;
  if (url.includes('/dependencies')) return [];
  if (url.includes('/sprints')) return [];
  if (url.includes('/agent-profiles')) return { protocol_version: 1, data: [PROFILE] };
  if (url.includes('/runners')) return { protocol_version: 1, data: oneRunner ? [{ runner_id: 'r1', state: 'active', online: true }] : [] };
  if (url.endsWith('/autonomy')) return { level: 'L3' };
  if (url.endsWith('/auto-start')) {
    return { enabled: autoStart, status: 'To Do', waiting: waitingReason ? [{ item_id: ITEM.id, why: waitingReason }] : [] };
  }
  if (url.includes('/projects/')) return { id: 'p1', auto_start_enabled: autoStart, auto_start_status: 'To Do', default_profile_id: 'prof-1' };
  if (url.includes('/events') || url.includes('/artifacts')) return { protocol_version: 1, data: [] };
  if (url.includes('/attempts')) return { protocol_version: 1, data: runs.length > 0 ? [ATTEMPT] : [] };
  if (url.includes('/executions')) return { protocol_version: 1, data: runs };
  return {};
}

beforeEach(() => {
  runs = [];
  projectItems = [];
  folderStatus = null;
  underItems = [];
  parentOptions = [];
  writes = [];
  localStorage.clear();
  vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL, init?: RequestInit) => {
    const method = init?.method ?? 'GET';
    const body = method === 'GET' ? null : JSON.parse(String(init?.body ?? 'null'));
    if (method !== 'GET') writes.push({ method, url: String(input), body });
    if (String(input).includes('/folder/status') && folderStatus === null) return Promise.resolve(new Response('{"error":{"status":404,"message":"no folder"}}', { status: 404 }));
    const answer = method === 'PATCH' ? { ...ITEM, ...body } : method === 'POST' ? { id: 'made', project_id: 'p1' } : jsonOf(String(input));
    return Promise.resolve(
      new Response(JSON.stringify(answer), {
        status: method === 'POST' ? 201 : 200,
        headers: { 'Content-Type': 'application/json', ETag: '"1"' },
      }),
    );
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function Body(props: { layout: 'drawer' | 'page' }) {
  const editor = useItemEditor(() => ITEM.id);
  const [tab, setTab] = createSignal('details');
  return (
    <Show when={editor.item()}>
      {(it) => <ItemBody item={it()} editor={editor} layout={props.layout} tab={tab()} onTabChange={setTab} />}
    </Show>
  );
}

async function mount(layout: 'drawer' | 'page') {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <ProjectContext.Provider value={projectValue}>
          <MemoryRouter>
            <Route path="/" component={() => <Body layout={layout} />} />
          </MemoryRouter>
        </ProjectContext.Provider>
      </ExecutionStoreProvider>
    ),
    container,
  );
  for (let i = 0; i < 12; i++) await flush();
  return { container, dispose };
}

const labels = (c: Element, sel: string) => [...c.querySelectorAll(sel)].map((e) => e.textContent?.trim());

describe('ItemBody', () => {
  it('reads top down: the title, the next step, the description, Done when, For the agent, the runs, the activity, then the Properties fold', async () => {
    const { container, dispose } = await mount('drawer');
    const order = [
      container.querySelector('[aria-label="Item title"]'),
      container.querySelector('[data-testid="now-panel"]'),
      container.querySelector('#item-description'),
      container.querySelector('#item-done-when'),
      container.querySelector('[data-fold="for-the-agent"]'),
      container.querySelector('[data-fold="runs"]'),
      container.querySelector('#item-activity'),
      container.querySelector('[data-fold="details"]'),
    ] as Element[];
    expect(order.every(Boolean)).toBe(true);
    for (let i = 1; i < order.length; i++) {
      expect(order[i - 1].compareDocumentPosition(order[i]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
    expect(container.querySelector('[role="tablist"]')).toBeNull();
    dispose();
  });

  it('the drawer says what is open about the item in chips under its title: the status, the priority and who has it', async () => {
    const { container, dispose } = await mount('drawer');
    const chips = container.querySelector('[data-testid="item-chips"]')!;
    expect(chips.querySelector('[data-testid="item-status"]')?.textContent).toBe('todo');
    expect(chips.querySelector('[data-testid="item-priority"]')?.textContent).toBe('High');
    expect(chips.querySelector('[data-testid="item-assignee"]')?.textContent).toBe('Ana');
    dispose();
  });

  it('the drawer folds For the agent to one line that opens it; the page shows it open in its own panel', async () => {
    const drawer = await mount('drawer');
    const head = drawer.container.querySelector<HTMLButtonElement>('[data-fold="for-the-agent"] > button')!;
    expect(head.getAttribute('aria-expanded')).toBe('false');
    expect(head.textContent).toContain('For the agent');
    head.click();
    await flush();
    expect(head.getAttribute('aria-expanded')).toBe('true');
    drawer.dispose();
    const page = await mount('page');
    const panel = page.container.querySelector('section[aria-labelledby="item-for-the-agent"]')!;
    expect(panel.textContent).toContain('Preview everything the agent will read');
    expect(page.container.querySelector('[data-fold="for-the-agent"]')).toBeNull();
    page.dispose();
  });

  it('says Not ready in the next step when the project list sends why, as given, and nothing when it does not', async () => {
    projectItems = [{ ...ITEM, brief_summary: { done_when: false, checks: 0, risk: null, not_ready: 'Not ready: no Done when' } }];
    const ready = await mount('page');
    expect(ready.container.querySelector('[data-testid="now-panel"] [data-testid="not-ready"]')?.textContent).toBe('Not ready: no Done when');
    ready.dispose();
    projectItems = [{ ...ITEM, brief_summary: { done_when: true, checks: 1, risk: 'low', not_ready: null } }];
    const fine = await mount('page');
    expect(fine.container.querySelector('[data-testid="not-ready"]')).toBeNull();
    fine.dispose();
    projectItems = [ITEM];
    const none = await mount('page');
    expect(none.container.querySelector('[data-testid="not-ready"]')).toBeNull();
    none.dispose();
  });

  it('in a project with a revision the plan fields say so, and a field the board changed says the next export writes it', async () => {
    Object.assign(ITEM, { key: 'my-item' });
    folderStatus = { last_revision: { number: 7, origin: 'board', hash: 'h', created_at: '2026-11-01T00:00:00Z' }, board_hash: 'b', changes: [] };
    try {
      const held = await mount('page');
      const marks = [...held.container.querySelectorAll('[data-testid="folder-mark"]')];
      expect(marks.length).toBeGreaterThan(0);
      expect(marks.every((m) => m.textContent === 'In the project folder')).toBe(true);
      expect(held.container.querySelector('#item-description')!.closest('section')!.textContent).toContain('In the project folder');
      held.dispose();

      folderStatus = {
        last_revision: { number: 7, origin: 'board', hash: 'h', created_at: '2026-11-01T00:00:00Z' },
        board_hash: 'b',
        changes: [{ section: 'items', key: 'my-item', name: 'My item', verb: 'update', fields: [{ path: 'description', from: 'a', to: 'b' }] }],
      };
      const changed = await mount('page');
      const section = changed.container.querySelector('#item-description')!.closest('section')!;
      expect(section.querySelector('[data-testid="folder-mark"]')!.textContent).toBe('Changed on the board since revision #7: the next export writes it');
      // The title is untouched: it still only says it is in the folder.
      expect(changed.container.querySelector('[aria-label="Item title"]')!.closest('div')!.parentElement!.querySelector('[data-testid="folder-mark"]')!.textContent).toBe('In the project folder');
      changed.dispose();
    } finally {
      delete (ITEM as Record<string, unknown>).key;
    }
  });

  it('a project with no folder says nothing about one', async () => {
    const { container, dispose } = await mount('page');
    expect(container.querySelector('[data-testid="folder-mark"]')).toBeNull();
    expect(container.querySelector('[data-testid="board-mark"]')).toBeNull();
    dispose();
  });

  it('an item under a group shows the groups above it as a path, outermost first', async () => {
    Object.assign(ITEM, { parent_id: 'story' });
    projectItems = [
      { ...ITEM, id: 'epic', title: 'Checkout', parent_id: null, item_type: 'epic', node: 'group' },
      { ...ITEM, id: 'story', title: 'Payments', parent_id: 'epic', node: 'group' },
      ITEM,
    ];
    try {
      const { container, dispose } = await mount('drawer');
      const path = container.querySelector('[data-testid="item-path"]')!;
      expect([...path.querySelectorAll('a')].map((a) => a.textContent)).toEqual(['Checkout', 'Payments']);
      expect(container.querySelector('[data-testid="item-parent"]')).toBeNull();
      dispose();
    } finally {
      delete (ITEM as Record<string, unknown>).parent_id;
    }
  });

  it("a group's page leads with Progress, open: its bar, its work under the Work states, and Add item and Plan with agent", async () => {
    Object.assign(ITEM, { item_type: 'epic', node: 'group', progress: { total: 2, done: 1, needs_you: 0, open_runs: 1, teams: ['saas-code'] } });
    underItems = [
      { ...ITEM, id: 'kid-1', title: 'Build the form', item_type: 'task', node: 'work', parent_id: ITEM.id, progress: null },
      { ...ITEM, id: 'kid-2', title: 'Write the copy', item_type: 'task', node: 'work', parent_id: ITEM.id, progress: null },
    ];
    runs = [{ ...RUN, item_id: 'kid-1' }];
    try {
      const { container, dispose } = await mount('page');
      const fold = container.querySelector<HTMLElement>('[data-testid="item-progress"]')!;
      expect(fold.querySelector('button[aria-expanded]')!.getAttribute('aria-expanded')).toBe('true');
      expect(fold.querySelector('[data-testid="progress-text"]')!.textContent).toBe('1 of 2 done');
      expect(fold.textContent).toContain('Teams: saas-code');
      const sections = [...fold.querySelectorAll<HTMLElement>('[data-testid="progress-section"]')];
      expect(sections.map((x) => x.dataset.state)).toEqual(['running', 'none']);
      expect([...sections[0].querySelectorAll('a')].map((a) => a.textContent)).toEqual(['Build the form']);
      expect([...sections[1].querySelectorAll('a')].map((a) => a.textContent)).toEqual(['Write the copy']);
      const buttons = [...container.querySelectorAll('button')].map((b) => b.textContent?.trim());
      expect(buttons).toContain('Add item');
      expect(buttons.filter((b) => b === 'Plan with agent')).toHaveLength(1);
      expect(buttons).not.toContain('Run with agent');
      // The progress comes before the next-step panel.
      expect(fold.compareDocumentPosition(container.querySelector('[data-testid="now-panel"]')!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(container.querySelector('[data-testid="now-panel"]')!.textContent).toContain('Nothing has planned this yet.');
      dispose();
    } finally {
      for (const key of ['progress', 'node']) delete (ITEM as Record<string, unknown>)[key];
      ITEM.item_type = 'task';
    }
  });

  it('Add item on a group creates an item under it', async () => {
    Object.assign(ITEM, { item_type: 'epic', node: 'group' });
    try {
      const { container, dispose } = await mount('page');
      [...container.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Add item')!.click();
      await flush();
      const input = container.querySelector<HTMLInputElement>('input[aria-label="New item title"]')!;
      input.value = 'Draft the terms';
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      await flush();
      await flush();
      expect(writes.map((w) => `${w.method} ${w.url}`)).toEqual([`POST /api/projects/p1/items`]);
      expect(writes[0].body).toEqual({ title: 'Draft the terms', parent_id: ITEM.id });
      dispose();
    } finally {
      for (const key of ['node']) delete (ITEM as Record<string, unknown>)[key];
      ITEM.item_type = 'task';
    }
  });

  it('work that was split says Split into n items above its run, and its run section reads like a group', async () => {
    Object.assign(ITEM, { node: 'group', progress: { total: 3, done: 0, needs_you: 0, open_runs: 0, teams: [] } });
    try {
      const { container, dispose } = await mount('page');
      expect(container.querySelector('[data-testid="item-progress"]')).toBeNull();
      const split = container.querySelector('[data-testid="item-split"]')!;
      expect(split.textContent?.trim()).toBe('Split into 3 items');
      expect(split.compareDocumentPosition(container.querySelector('[data-testid="now-panel"]')!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(container.querySelector('[data-testid="now-panel"]')!.textContent).toContain('Nothing has planned this yet.');
      dispose();
    } finally {
      for (const key of ['progress', 'node']) delete (ITEM as Record<string, unknown>)[key];
    }
  });

  it('Move under… in the header menu offers the parents the server admits and moves the item under the one chosen', async () => {
    parentOptions = [{ id: 'epic', title: 'Checkout', type: 'epic' }];
    const { container, dispose } = await mount('page');
    container.querySelector<HTMLButtonElement>('button[aria-label="More actions"]')!.click();
    await flush();
    [...container.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((b) => b.textContent === 'Move under…')!.click();
    await flush();
    await flush();
    const option = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find((o) => o.textContent?.includes('Checkout'))!;
    expect(fetch).toHaveBeenCalledWith(expect.stringContaining('/projects/p1/parents?item=item-1'), expect.anything());
    option.click();
    await flush();
    await flush();
    expect(writes.find((w) => w.method === 'PATCH')?.body).toEqual({ parent_id: 'epic' });
    dispose();
  });

  it('an item with no run says who would do it, under its one Run with agent button', async () => {
    const { container, dispose } = await mount('drawer');
    expect(container.querySelector('[data-testid="now-who"]')?.textContent).toBe('Would go to Code agent · team saas-code');
    expect([...container.querySelectorAll('button')].filter((b) => b.textContent === 'Run with agent')).toHaveLength(1);
    expect(container.querySelector('[role="tab"] [role="img"]')).toBeNull();
    dispose();
  });

  it('the Properties fold is shut with the priority, assignee and due date in its header, and the Automation line is shut with its sentence', async () => {
    const { container, dispose } = await mount('drawer');
    const details = container.querySelector<HTMLButtonElement>('[data-fold="details"] > button')!;
    expect(details.getAttribute('aria-expanded')).toBe('false');
    expect(details.textContent).toContain('Properties');
    expect(details.textContent).toContain('High · Ana');
    details.click();
    await flush();
    expect(container.querySelector('[data-fold="details"] section[aria-label="Properties"]')).not.toBeNull();
    expect(container.querySelector('[data-fold="details"] section[aria-label="Related"]')).not.toBeNull();
    const automation = container.querySelector<HTMLButtonElement>('[data-fold="automation"] > button')!;
    expect(automation.getAttribute('aria-expanded')).toBe('false');
    expect(automation.textContent).toContain("By hand: this project doesn't start tasks by itself");
    dispose();
  });

  it('the Runs section carries the latest run state as a dot with a label, and no Run again is offered beside a live run', async () => {
    runs = [RUN];
    const { container, dispose } = await mount('drawer');
    const head = container.querySelector('[data-fold="runs"] > button')!;
    expect(head.textContent).toContain('Runs');
    expect(head.querySelector('[role="img"]')?.getAttribute('aria-label')).toBe('Running');
    expect([...container.querySelectorAll('button')].filter((b) => b.textContent === 'Run again')).toHaveLength(0);
    dispose();
  });

  it('the Runs section is shut until a person opens it', async () => {
    const shut = await mount('drawer');
    const head = shut.container.querySelector<HTMLButtonElement>('[data-fold="runs"] > button')!;
    expect(head.getAttribute('aria-expanded')).toBe('false');
    head.click();
    await flush();
    expect(head.getAttribute('aria-expanded')).toBe('true');
    shut.dispose();
  });

  it('the Automation line is the next-step panel\'s footer, not a property', async () => {
    const { container, dispose } = await mount('drawer');
    const row = container.querySelector('[data-testid="automation-row"]')!;
    expect(container.querySelector('[data-testid="now-panel"]')!.lastElementChild).toBe(row);
    expect(row.textContent).toContain("By hand: this project doesn't start tasks by itself");
    expect(row.querySelector('button[aria-label="Help: Automation"]')).not.toBeNull();
    dispose();
  });

  it('who would do the task is said once: by the idle next-step panel before a run, by the Automation line (its team) after one', async () => {
    autoStart = true;
    try {
      const idle = await mount('drawer');
      expect(idle.container.querySelector('[data-testid="now-who"]')?.textContent).toBe('Would go to Code agent · team saas-code');
      const row = idle.container.querySelector('[data-testid="automation-row"]')!.textContent!;
      expect(row).toContain('it would start when it reaches To Do');
      expect(row).not.toContain('Goes to');
      expect(row).not.toContain('· Team saas-code');
      idle.dispose();
      runs = [RUN];
      const ran = await mount('drawer');
      expect(ran.container.querySelector('[data-testid="now-who"]')).toBeNull();
      expect(ran.container.querySelector('[data-testid="automation-row"]')!.textContent).toContain('· Team saas-code');
      ran.dispose();
    } finally {
      autoStart = false;
    }
  });

  it('says on the item why auto-start has not started it, once, and says nothing when nothing holds it back', async () => {
    autoStart = true;
    oneRunner = true;
    waitingReason = { code: 'max_open', text: '3 runs are already open, the most this project allows' };
    try {
      const held = await mount('drawer');
      expect(held.container.querySelector('[data-testid="now-waiting"]')?.textContent).toBe(
        'Waiting to start: 3 runs are already open, the most this project allows',
      );
      // The idle next-step panel says it, so the Automation line under it does not say it twice.
      expect(held.container.querySelector('[data-testid="automation-row"]')!.textContent).not.toContain('Waiting to start');
      held.dispose();
      waitingReason = null;
      const free = await mount('drawer');
      expect(free.container.querySelector('[data-testid="now-waiting"]')).toBeNull();
      expect(free.container.querySelector('[data-testid="automation-row"]')!.textContent).not.toContain('Waiting to start');
      free.dispose();
    } finally {
      autoStart = false;
      oneRunner = false;
      waitingReason = null;
    }
  });

  it('Propose items from a file is in the ⋯ menu, not on the screen until chosen, and opens the Runs section', async () => {
    const { container, dispose } = await mount('drawer');
    expect(container.querySelector('section[aria-label="Propose items from a file"]')).toBeNull();
    container.querySelector<HTMLButtonElement>('button[aria-label="More actions"]')!.click();
    await flush();
    const entry = [...container.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((b) => b.textContent === 'Propose items from a file')!;
    entry.click();
    await flush();
    expect(container.querySelector('[role="menu"]')).toBeNull();
    await flush();
    expect(container.querySelector('[data-fold="runs"] > button')!.getAttribute('aria-expanded')).toBe('true');
    const section = container.querySelector('section[aria-label="Propose items from a file"]')!;
    expect(section.querySelector('input[type="file"]')).not.toBeNull();
    [...section.querySelectorAll('button')].find((b) => b.textContent === 'Close')!.click();
    await flush();
    expect(container.querySelector('section[aria-label="Propose items from a file"]')).toBeNull();
    dispose();
  });

  it('the page keeps its side column of properties and what the item is related to, with no Properties fold', async () => {
    const { container, dispose } = await mount('page');
    expect(container.querySelector('[data-fold="details"]')).toBeNull();
    const properties = container.querySelector('aside section[aria-label="Properties"]')!;
    expect(properties.textContent).toContain('Priority');
    expect(properties.textContent).toContain('Status');
    expect(container.querySelector('aside section[aria-label="Related"]')!.textContent).toContain('Parent');
    expect(container.querySelector('[data-testid="now-panel"]')!.closest('aside')).toBeNull();
    dispose();
  });
});
