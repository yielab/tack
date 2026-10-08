import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../../shared/state/projectContext';
import type { Resource } from 'solid-js';
import type { Project } from '../../../shared/types';
import GeneralPanel from './GeneralPanel';
import RolesPanel from './RolesPanel';
import DataPanel from './DataPanel';
import AutomationPanel from './AutomationPanel';
import { toast } from '../../../shared/ui/toast';

const PROJECT = {
  id: 'p1',
  name: 'Project One',
  description: 'desc',
  project_type: 'software',
  vocabulary: {},
  workflow: { workflow_type: 'kanban', statuses: [{ name: 'To Do', order: 0, category: 'todo' }, { name: 'Done', order: 1, category: 'done' }] },
  archived: false,
  code_origin: 'folder',
  repository: '/tmp/repo',
  default_branch: 'main',
  workspace_mode: 'local_branch',
  push_after_run: false,
  default_harness: null,
  default_profile_id: null,
  on_finish_status: null,
} as unknown as Project;

const ctx: ProjectContextValue = {
  projectId: () => 'p1',
  project: (() => PROJECT) as unknown as Resource<Project>,
  workflow: () => PROJECT.workflow,
  vocabulary: () => ({}),
  refetch: () => {},
};

const flush = () => new Promise((r) => setTimeout(r, 0));
let fetchMock: ReturnType<typeof vi.spyOn>;

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

beforeEach(() => {
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = String(input);
    if (url.endsWith('/api/runners')) {
      const snapshot = { harnesses: [{ harness_kind: 'codex' }, { harness_kind: 'claude-code', native_provider: 'anthropic' }], push_configured: true };
      return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [{ runner_id: 'r1', name: 'Local', state: 'active', capability_snapshot: snapshot }] }), { status: 200, headers: { 'Content-Type': 'application/json' } }));
    }
    if (url.endsWith('/api/agent-profiles')) {
      return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [{ agent_profile_id: 'prof1', name: 'Implementer' }] }), { status: 200, headers: { 'Content-Type': 'application/json' } }));
    }
    if (url.endsWith('/api/local-runner/check-folder')) {
      return Promise.resolve(new Response(JSON.stringify({ exists: true, is_dir: true, is_git: true, branch: 'main', remote_url: null, dirty_files: 3 }), { status: 200, headers: { 'Content-Type': 'application/json' } }));
    }
    if (url.includes('/export')) return Promise.resolve(new Response('x', { status: 200 }));
    if (url.endsWith('/api/projects/p1/roles')) return Promise.resolve(new Response('[]', { status: 200 }));
    if (url.endsWith('/api/projects/import')) return Promise.resolve(new Response(JSON.stringify({ id: 'new' }), { status: 200 }));
    if (url.endsWith('/api/projects/p1/import-csv')) return Promise.resolve(new Response(JSON.stringify({ created: 3, skipped: 0 }), { status: 200, headers: { 'Content-Type': 'application/json' } }));
    return Promise.resolve(new Response(JSON.stringify({ id: 'p1' }), { status: 200 }));
  });
});

function mount(comp: () => unknown) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={ctx}>
        <MemoryRouter>
          <Route path="/" component={() => comp() as never} />
        </MemoryRouter>
      </ProjectContext.Provider>
    ),
    container,
  );
  return { container, dispose };
}

const btn = (c: HTMLElement, text: string) =>
  Array.from(c.querySelectorAll('button')).find((b) => b.textContent?.trim() === text)!;

describe('Settings panels', () => {
  it('GeneralPanel saves via PATCH /projects/{id}', async () => {
    const { container } = mount(() => <GeneralPanel />);
    await flush();
    btn(container, 'Save').click();
    await flush();
    const patch = fetchMock.mock.calls.find(
      (c) => (c[1] as RequestInit)?.method === 'PATCH' && String(c[0]).endsWith('/api/projects/p1'),
    );
    expect(patch).toBeTruthy();
    expect(JSON.parse((patch![1] as RequestInit).body as string)).toMatchObject({ name: 'Project One' });
  });

  it('RolesPanel adds a role via POST /projects/{id}/roles', async () => {
    const { container } = mount(() => <RolesPanel />);
    await flush();
    const nameInput = container.querySelector<HTMLInputElement>('input:not([type="color"])')!;
    nameInput.value = 'Designer';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    container.querySelector('form')!.dispatchEvent(new Event('submit'));
    await flush();
    expect(
      fetchMock.mock.calls.some(
        (c) => (c[1] as RequestInit)?.method === 'POST' && String(c[0]).endsWith('/api/projects/p1/roles'),
      ),
    ).toBe(true);
  });

  it('DataPanel exports (GET /export) and imports (POST /projects/import)', async () => {
    const { container } = mount(() => <DataPanel />);
    await flush();

    btn(container, 'Export JSON').click();
    await vi.waitFor(() => {
      expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/api/projects/p1/export?format=json'))).toBe(true);
      expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
    });
    const exported = vi.mocked(URL.createObjectURL).mock.calls[0][0];
    expect(Object.prototype.toString.call(exported)).toBe('[object Blob]');
    expect(exported.size).toBe(1);
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:vitest/1');

    const input = container.querySelector<HTMLInputElement>('input[type="file"]')!;
    const file = new File([JSON.stringify({ project: {}, items: [] })], 'p.json', { type: 'application/json' });
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    input.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect(
      fetchMock.mock.calls.some(
        (c) => (c[1] as RequestInit)?.method === 'POST' && String(c[0]).endsWith('/api/projects/import'),
      ),
    ).toBe(true);
  });

  it('DataPanel CSV import POSTs to /projects/{id}/import-csv', async () => {
    const { container, dispose } = mount(() => <DataPanel />);
    await flush();

    // The CSV file input is the second input[type="file"] (JSON import is the first).
    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="file"]');
    const csvInput = inputs[1];
    const csvText = 'title\nFoo\nBar\nBaz';
    const file = new File([csvText], 'items.csv', { type: 'text/csv' });
    Object.defineProperty(csvInput, 'files', { value: [file], configurable: true });
    csvInput.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    await flush(); // second flush for file.text() async resolution

    expect(
      fetchMock.mock.calls.some(
        (c) =>
          (c[1] as RequestInit)?.method === 'POST' &&
          String(c[0]).endsWith('/api/projects/p1/import-csv'),
      ),
    ).toBe(true);
    dispose();
  });

  it('automation_panel_saves_each_field_on_change', async () => {
    (PROJECT as { default_harness: string | null }).default_harness = 'claude-code';
    const { container } = mount(() => <AutomationPanel />);
    await flush();
    await flush();
    const byText = (sel: string, text: string) =>
      Array.from(container.querySelectorAll<HTMLElement>(sel)).find((e) => e.textContent?.trim() === text)!;
    const lastPatch = () => {
      const calls = fetchMock.mock.calls.filter(
        (c) => (c[1] as RequestInit)?.method === 'PATCH' && String(c[0]).endsWith('/api/projects/p1'),
      );
      return JSON.parse((calls[calls.length - 1][1] as RequestInit).body as string);
    };
    const folder = container.querySelector<HTMLInputElement>('input[aria-label="Folder"]')!;
    const select = container.querySelector<HTMLSelectElement>('select')!;
    const push = container.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    const rows: [string, () => void, object][] = [
      ['folder', () => { folder.value = '/tmp/other'; folder.dispatchEvent(new Event('change', { bubbles: true })); }, { repository: '/tmp/other', default_branch: 'main' }],
      ['mode', () => byText('label', 'In the folder, directly').querySelector('input')!.click(), { workspace_mode: 'in_place' }],
      ['push', () => push.click(), { push_after_run: true }],
      ['finish', () => { select.value = 'Done'; select.dispatchEvent(new Event('change', { bubbles: true })); }, { on_finish_status: 'Done' }],
      ['harness', () => byText('button', 'Codex').click(), { default_harness: 'codex' }],
      ['profile', () => byText('button', 'Implementer').click(), { default_profile_id: 'prof1' }],
    ];
    const dod = container.querySelector<HTMLTextAreaElement>('textarea[aria-label="Definition of done"]')!;
    rows.push(
      ['specific model', () => {
        byText('label', 'Specific model').querySelector('input')!.click();
        const model = container.querySelector<HTMLSelectElement>('select[aria-label="Model"]')!;
        model.value = 'claude-opus-5-5';
        model.dispatchEvent(new Event('change', { bubbles: true }));
      }, { default_model: { kind: 'explicit', provider: 'anthropic', model_id: 'claude-opus-5-5' } }],
      ['auto model', () => byText('label', "The agent's default (recommended)").querySelector('input')!.click(), { default_model: { kind: 'auto' } }],
      ['definition of done', () => { dod.value = 'Tests pass'; dod.dispatchEvent(new Event('blur')); }, { definition_of_done: 'Tests pass' }],
    );
    try {
      for (const [name, act, expected] of rows) {
        act();
        await flush();
        await flush();
        expect(lastPatch(), name).toMatchObject(expected);
      }
    } finally {
      (PROJECT as { default_harness: string | null }).default_harness = null;
    }
  });

  it('automation_panel_shows_a_rejected_folder_under_the_field', async () => {
    const base = fetchMock.getMockImplementation()!;
    fetchMock.mockImplementation((input, init) =>
      (init as RequestInit)?.method === 'PATCH'
        ? Promise.resolve(new Response(JSON.stringify({ error: { status: 400, message: 'That folder does not exist', details: { field: 'repository' } } }), { status: 400, headers: { 'Content-Type': 'application/json' } }))
        : base(input, init),
    );
    const toastError = vi.spyOn(toast, 'error');
    const { container } = mount(() => <AutomationPanel />);
    await flush();
    await flush();
    const folder = container.querySelector<HTMLInputElement>('input[aria-label="Folder"]')!;
    folder.value = '/nope';
    folder.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    await flush();
    expect(container.textContent).toContain('That folder does not exist');
    expect(toastError).not.toHaveBeenCalled();
  });
});
