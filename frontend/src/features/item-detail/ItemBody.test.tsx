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

const projectValue: ProjectContextValue = {
  projectId: () => 'p1',
  project: (() => null) as unknown as Resource<null>,
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
let autoStart = false;
/** Why auto-start has not started the item, as the server answers it; null lists nothing. */
let oneRunner = false;
let waitingReason: { code: string; text: string } | null = null;

function jsonOf(url: string): unknown {
  if (url.endsWith(`/api/items/${ITEM.id}`)) return { item: ITEM, roles: [], dependencies: [] };
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
  localStorage.clear();
  vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL) =>
    Promise.resolve(
      new Response(JSON.stringify(jsonOf(String(input))), {
        status: 200,
        headers: { 'Content-Type': 'application/json', ETag: '"1"' },
      }),
    ),
  );
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
  for (let i = 0; i < 8; i++) await flush();
  return { container, dispose };
}

const labels = (c: Element, sel: string) => [...c.querySelectorAll(sel)].map((e) => e.textContent?.trim());

describe('ItemBody', () => {
  it('leads with the header, then the Now panel, then the tabs, then the Details fold', async () => {
    const { container, dispose } = await mount('drawer');
    const order = [
      container.querySelector('[aria-label="Item title"]'),
      container.querySelector('[data-testid="now-panel"]'),
      container.querySelector('[role="tablist"]'),
      container.querySelector('[data-testid="details-fold"]'),
    ] as Element[];
    expect(order.every(Boolean)).toBe(true);
    for (let i = 1; i < order.length; i++) {
      expect(order[i - 1].compareDocumentPosition(order[i]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
    expect(labels(container, '[role="tab"]')).toEqual(['Overview', 'Execution', 'Activity', 'Dependencies0', 'Files', 'Fields']);
    dispose();
  });

  it('an item with no run says who would do it, under its one Run with agent button', async () => {
    const { container, dispose } = await mount('drawer');
    expect(container.querySelector('[data-testid="now-who"]')?.textContent).toBe('Would go to Code agent · team saas-code');
    expect([...container.querySelectorAll('button')].filter((b) => b.textContent === 'Run with agent')).toHaveLength(1);
    expect(container.querySelector('[role="tab"] [role="img"]')).toBeNull();
    dispose();
  });

  it('the Execution tab carries the latest run state as a dot with a label, and no Run again is offered beside a live run', async () => {
    runs = [RUN];
    const { container, dispose } = await mount('drawer');
    const tab = [...container.querySelectorAll('[role="tab"]')].find((t) => t.textContent?.startsWith('Execution'))!;
    expect(tab.querySelector('[role="img"]')?.getAttribute('aria-label')).toBe('Running');
    expect([...container.querySelectorAll('button')].filter((b) => b.textContent === 'Run again')).toHaveLength(0);
    dispose();
  });

  it('the Automation line is the Now panel\'s footer, not a property', async () => {
    const { container, dispose } = await mount('drawer');
    const row = container.querySelector('[data-testid="automation-row"]')!;
    expect(container.querySelector('[data-testid="now-panel"]')!.lastElementChild).toBe(row);
    expect(row.textContent).toContain("By hand: this project doesn't start tasks by itself");
    expect(row.querySelector('button[aria-label="Help: Automation"]')).not.toBeNull();
    dispose();
  });

  it('who would do the task is said once: by the idle Now panel before a run, by the Automation line (its team) after one', async () => {
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

  it('says on the item why auto-start has not started it, in the Now panel and the Automation line, and says nothing when nothing holds it back', async () => {
    autoStart = true;
    oneRunner = true;
    waitingReason = { code: 'wip', text: 'In Progress is full (5 of 5): move something on to make room' };
    try {
      const held = await mount('drawer');
      expect(held.container.querySelector('[data-testid="now-waiting"]')?.textContent).toBe(
        'Waiting to start: In Progress is full (5 of 5): move something on to make room',
      );
      expect(held.container.querySelector('[data-testid="automation-row"]')!.textContent).toContain(
        'Waiting to start: In Progress is full (5 of 5): move something on to make room',
      );
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

  it('Propose items from a file is in the ⋯ menu, not on the screen until chosen, and opens on the Execution tab', async () => {
    const { container, dispose } = await mount('drawer');
    expect(container.querySelector('section[aria-label="Propose items from a file"]')).toBeNull();
    container.querySelector<HTMLButtonElement>('button[aria-label="More actions"]')!.click();
    await flush();
    const entry = [...container.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((b) => b.textContent === 'Propose items from a file')!;
    entry.click();
    await flush();
    expect(container.querySelector('[role="menu"]')).toBeNull();
    expect(container.querySelector('[role="tab"][aria-selected="true"]')?.textContent).toContain('Execution');
    const section = container.querySelector('section[aria-label="Propose items from a file"]')!;
    expect(section.querySelector('input[type="file"]')).not.toBeNull();
    [...section.querySelectorAll('button')].find((b) => b.textContent === 'Close')!.click();
    await flush();
    expect(container.querySelector('section[aria-label="Propose items from a file"]')).toBeNull();
    dispose();
  });

  it('the page keeps its side column of properties, with no Details fold', async () => {
    const { container, dispose } = await mount('page');
    expect(container.querySelector('[data-testid="details-fold"]')).toBeNull();
    const properties = container.querySelector('aside section[aria-label="Properties"]')!;
    expect(properties.textContent).toContain('Priority');
    expect(container.querySelector('[data-testid="now-panel"]')!.closest('aside')).toBeNull();
    dispose();
  });
});
