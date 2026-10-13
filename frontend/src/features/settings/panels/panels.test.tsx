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
    if (url.includes('/api/projects/p1/import-csv')) {
      const result = { dry_run: url.includes('dry_run'), created: 3, skipped: 0, dependencies: 0, warnings: [], items: [] };
      return Promise.resolve(new Response(JSON.stringify(result), { status: 200, headers: { 'Content-Type': 'application/json' } }));
    }
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

/** The labels the "Specific model" select offers, in order, without the placeholder. */
const modelLabels = (c: HTMLElement) =>
  Array.from(c.querySelector<HTMLSelectElement>('select[aria-label="Model"]')!.options)
    .map((o) => o.textContent)
    .filter((label) => label !== 'Choose a model');

/** Picks a model by its visible label: the option values are the panel's own encoding, never typed here. */
const chooseModel = (c: HTMLElement, label: string) => {
  const model = c.querySelector<HTMLSelectElement>('select[aria-label="Model"]')!;
  const option = Array.from(model.options).find((o) => o.textContent === label);
  if (!option) throw new Error(`no model option labelled ${label}`);
  model.value = option.value;
  model.dispatchEvent(new Event('change', { bubbles: true }));
};

/** A `/api/runners` body whose one runner reports `harnesses`. */
const runnersWith = (harnesses: unknown[]) =>
  new Response(
    JSON.stringify({ protocol_version: 1, data: [{ runner_id: 'r1', name: 'Local', state: 'active', capability_snapshot: { harnesses, push_configured: true } }] }),
    { status: 200, headers: { 'Content-Type': 'application/json' } },
  );

const GATEWAY_CATALOG = {
  harness_kind: 'docket',
  native_provider: 'docket',
  model_combinations: [
    { model_provider: 'vercel-ai-gateway', model_ids: ['anthropic/claude-sonnet-4-5', 'openai/gpt-5'], discovery: 'probed' },
  ],
};

const lastModelPatch = () => {
  const calls = fetchMock.mock.calls.filter(
    (c) => (c[1] as RequestInit)?.method === 'PATCH' && String(c[0]).endsWith('/api/projects/p1'),
  );
  return JSON.parse((calls[calls.length - 1][1] as RequestInit).body as string);
};

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

  it('data_panel_file_import_previews_then_writes_on_confirm', async () => {
    const { container, dispose } = mount(() => <DataPanel />);
    await flush();

    // Inputs: the snapshot restore, then "Add items", then "New project".
    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="file"]');
    const file = new File(['title\nFoo\nBar\nBaz'], 'items.csv', { type: 'text/csv' });
    Object.defineProperty(inputs[1], 'files', { value: [file], configurable: true });
    inputs[1].dispatchEvent(new Event('change', { bubbles: true }));

    const csvPosts = () =>
      fetchMock.mock.calls
        .filter((c) => (c[1] as RequestInit)?.method === 'POST' && String(c[0]).includes('/api/projects/p1/import-csv'))
        .map((c) => String(c[0]));
    await vi.waitFor(() => expect(csvPosts()).toEqual(['/api/projects/p1/import-csv?dry_run=true']));
    await vi.waitFor(() => expect(document.body.textContent).toContain('3 items'));

    btn(document.body, 'Add items').click();
    await vi.waitFor(() =>
      expect(csvPosts()).toEqual(['/api/projects/p1/import-csv?dry_run=true', '/api/projects/p1/import-csv']),
    );
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
        chooseModel(container, 'anthropic / claude-opus-5-5');
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

  it('automation_panel_offers_a_docket_gateway_catalog_as_provider_model_pairs_and_saves_the_pair', async () => {
    const base = fetchMock.getMockImplementation()!;
    fetchMock.mockImplementation((input, init) =>
      String(input).endsWith('/api/runners') ? Promise.resolve(runnersWith([GATEWAY_CATALOG])) : base(input, init),
    );
    (PROJECT as { default_harness: string | null }).default_harness = 'docket';
    try {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      Array.from(container.querySelectorAll('label')).find((l) => l.textContent?.trim() === 'Specific model')!.querySelector('input')!.click();
      await flush();
      expect(modelLabels(container)).toEqual(['vercel-ai-gateway / anthropic/claude-sonnet-4-5', 'vercel-ai-gateway / openai/gpt-5']);
      expect(container.textContent).not.toContain('No model ids are measured for this agent yet');
      chooseModel(container, 'vercel-ai-gateway / anthropic/claude-sonnet-4-5');
      await flush();
      await flush();
      expect(lastModelPatch()).toEqual({
        default_model: { kind: 'explicit', provider: 'vercel-ai-gateway', model_id: 'anthropic/claude-sonnet-4-5' },
      });
    } finally {
      (PROJECT as { default_harness: string | null }).default_harness = null;
    }
  });

  it('automation_panel_says_whether_the_default_agent_runs_on_this_computer', async () => {
    const base = fetchMock.getMockImplementation()!;
    let docket: unknown = { ...GATEWAY_CATALOG, sandbox: 'none', decisions: { support: 'supported' } };
    fetchMock.mockImplementation((input, init) =>
      String(input).endsWith('/api/runners') ? Promise.resolve(runnersWith([docket])) : base(input, init),
    );
    (PROJECT as { default_harness: string | null }).default_harness = 'docket';
    const line = (container: HTMLElement) => container.querySelector('[data-testid="default-agent-safety"]')?.textContent ?? '';
    try {
      const asks = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      expect(line(asks.container)).toContain('Not sandboxed on this computer, so the agent asks before each action');
      asks.dispose();
      asks.container.remove();
      docket = { ...GATEWAY_CATALOG, sandbox: 'none', decisions: { support: 'unsupported' } };
      const blocked = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      expect(line(blocked.container)).toContain("Doesn't run on this computer: it isn't sandboxed and can't ask before it acts");
    } finally {
      (PROJECT as { default_harness: string | null }).default_harness = null;
    }
  });

  it('automation_panel_shows_the_saved_pair_selected_whichever_provider_it_names', async () => {
    const base = fetchMock.getMockImplementation()!;
    fetchMock.mockImplementation((input, init) =>
      String(input).endsWith('/api/runners') ? Promise.resolve(runnersWith([GATEWAY_CATALOG])) : base(input, init),
    );
    const mutable = PROJECT as { default_harness: string | null; default_model: unknown };
    mutable.default_harness = 'docket';
    mutable.default_model = { kind: 'explicit', provider: 'vercel-ai-gateway', model_id: 'openai/gpt-5' };
    try {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const model = container.querySelector<HTMLSelectElement>('select[aria-label="Model"]')!;
      expect(model.selectedOptions[0]?.textContent).toBe('vercel-ai-gateway / openai/gpt-5');
    } finally {
      mutable.default_harness = null;
      mutable.default_model = undefined;
    }
  });

  it('automation_panel_keeps_the_static_list_under_the_native_provider_for_claude_code', async () => {
    (PROJECT as { default_harness: string | null }).default_harness = 'claude-code';
    try {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      Array.from(container.querySelectorAll('label')).find((l) => l.textContent?.trim() === 'Specific model')!.querySelector('input')!.click();
      await flush();
      const labels = modelLabels(container);
      expect(labels[0]).toBe('anthropic / claude-sonnet-5-5');
      expect(labels).toContain('anthropic / claude-opus-5-5');
      expect(labels.every((l) => l!.startsWith('anthropic / '))).toBe(true);
    } finally {
      (PROJECT as { default_harness: string | null }).default_harness = null;
    }
  });

  it('automation_panel_says_no_model_ids_are_measured_only_when_the_merged_list_is_empty', async () => {
    (PROJECT as { default_harness: string | null }).default_harness = 'codex';
    const base = fetchMock.getMockImplementation()!;
    fetchMock.mockImplementation((input, init) =>
      String(input).endsWith('/api/runners')
        ? Promise.resolve(runnersWith([{ harness_kind: 'codex', native_provider: 'openai', model_combinations: [] }]))
        : base(input, init),
    );
    try {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      Array.from(container.querySelectorAll('label')).find((l) => l.textContent?.trim() === 'Specific model')!.querySelector('input')!.click();
      await flush();
      expect(modelLabels(container)).toEqual([]);
      expect(container.querySelector<HTMLSelectElement>('select[aria-label="Model"]')!.disabled).toBe(true);
      expect(container.textContent).toContain('No model ids are measured for this agent yet; its default is used.');
    } finally {
      (PROJECT as { default_harness: string | null }).default_harness = null;
    }
  });

  it('automation_panel_offers_through_the_gateway_only_when_a_runner_reports_a_gateway_key', async () => {
    (PROJECT as { default_harness: string | null }).default_harness = 'claude-code';
    const base = fetchMock.getMockImplementation()!;
    const runnerWith = (providers: string[]) =>
      fetchMock.mockImplementation((input, init) =>
        String(input).endsWith('/api/runners')
          ? Promise.resolve(runnersWith([{ harness_kind: 'claude-code', native_provider: 'anthropic', providers }]))
          : base(input, init),
      );
    try {
      runnerWith([]);
      let view = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      Array.from(view.container.querySelectorAll('label')).find((l) => l.textContent?.trim() === 'Specific model')!.querySelector('input')!.click();
      await flush();
      expect(view.container.querySelector<HTMLSelectElement>('select[aria-label="Through"]')!.disabled).toBe(true);
      expect(view.container.textContent).toContain('add a gateway key on the Agents page');
      document.body.innerHTML = '';

      runnerWith(['vercel-ai-gateway']);
      view = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      Array.from(view.container.querySelectorAll('label')).find((l) => l.textContent?.trim() === 'Specific model')!.querySelector('input')!.click();
      await flush();
      const through = view.container.querySelector<HTMLSelectElement>('select[aria-label="Through"]')!;
      expect(through.disabled).toBe(false);
      through.value = 'vercel-ai-gateway';
      through.dispatchEvent(new Event('change', { bubbles: true }));
      await flush();
      chooseModel(view.container, 'vercel-ai-gateway / claude-opus-5-5');
      await flush();
      await flush();
      expect(lastModelPatch()).toMatchObject({ default_model: { kind: 'explicit', provider: 'vercel-ai-gateway', model_id: 'claude-opus-5-5' } });
    } finally {
      (PROJECT as { default_harness: string | null }).default_harness = null;
    }
  });
});
