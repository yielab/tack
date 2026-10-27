import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route, useNavigate } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../../shared/state/projectContext';
import type { Resource } from 'solid-js';
import type { Project } from '../../../shared/types';
import GeneralPanel from './GeneralPanel';
import RolesPanel from './RolesPanel';
import DataPanel from './DataPanel';
import ProjectSettings from '../ProjectSettings';
import AutomationPanel from './AutomationPanel';
import { HELP } from '../../../shared/help/texts';
import { toast } from '../../../shared/ui/toast';
import { flush } from '../../../test/flush';

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

const AUTONOMY = {
  level: 'L3',
  domains: [{ name: 'docs', path_globs: ['docs/**'], item_types: ['task'], level: 'L4', merged: 4, undone: 1 }],
  verify_require: [] as string[],
  attestation_public_key: null as string | null,
  verify_loop_max_attempts: 2,
  budgets: { tokens_per_day: null, auto_merges_per_day: null, queue_items_max: null },
  merge_on_done: false,
  automatic_merge_blocked_by: [] as string[],
};
let autonomy: Record<string, unknown> = AUTONOMY;
let roles: unknown[] = [];
/** Endings of the URLs that answer 500 until the test clears the list. */
let failing: string[] = [];
const autonomyPuts = () =>
  fetchMock.mock.calls
    .filter((c) => (c[1] as RequestInit)?.method === 'PUT' && String(c[0]).endsWith('/api/projects/p1/autonomy'))
    .map((c) => JSON.parse((c[1] as RequestInit).body as string));
const withInput = (c: HTMLElement, label: string) => c.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;
let fetchMock: ReturnType<typeof vi.spyOn>;

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

beforeEach(() => {
  autonomy = structuredClone(AUTONOMY);
  roles = [];
  failing = [];
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    if (failing.some((ending) => url.endsWith(ending))) {
      return Promise.resolve(new Response(JSON.stringify({ error: 'The board is unreachable' }), { status: 500, headers: { 'Content-Type': 'application/json' } }));
    }
    if (url.endsWith('/api/projects/p1/autonomy')) {
      if (init?.method === 'PUT') autonomy = JSON.parse(String(init.body));
      return Promise.resolve(new Response(JSON.stringify(autonomy), { status: 200, headers: { 'Content-Type': 'application/json' } }));
    }
    if (url.endsWith('/api/runners')) {
      const snapshot = {
        harnesses: [
          { harness_kind: 'codex' },
          { harness_kind: 'claude-code', models: { source: 'listed', ids: ['claude-sonnet-5-5', 'claude-opus-5-5'], reason: null } },
        ],
        push_configured: true,
      };
      return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [{ runner_id: 'r1', name: 'Local', state: 'active', capability_snapshot: snapshot }] }), { status: 200, headers: { 'Content-Type': 'application/json' } }));
    }
    if (url.endsWith('/api/agent-profiles')) {
      return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [{ agent_profile_id: 'prof1', name: 'Implementer', tool_policy: { docket: { team: 'saas-content' } } }] }), { status: 200, headers: { 'Content-Type': 'application/json' } }));
    }
    if (url.endsWith('/api/local-runner/check-folder')) {
      return Promise.resolve(new Response(JSON.stringify({ exists: true, is_dir: true, is_git: true, branch: 'main', remote_url: null, dirty_files: 3 }), { status: 200, headers: { 'Content-Type': 'application/json' } }));
    }
    if (url.includes('/export')) return Promise.resolve(new Response('x', { status: 200 }));
    if (url.endsWith('/api/projects/p1/roles')) return Promise.resolve(new Response(JSON.stringify(roles), { status: 200 }));
    if (url.endsWith('/api/projects/import')) return Promise.resolve(new Response(JSON.stringify({ id: 'new' }), { status: 200 }));
    if (url.includes('/api/projects/p1/import-csv')) {
      const result = { dry_run: url.includes('dry_run'), created: 3, skipped: 0, dependencies: 0, warnings: [], items: [], sections: [{ section: 'roles', created: 2, updated: 5, not_mentioned: 3, warnings: ['Planner is built in and was not changed'] }] };
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

/** The labels the model select offers, in order. */
const modelLabels = (c: HTMLElement) =>
  Array.from(c.querySelector<HTMLSelectElement>('select[aria-label="Model"]')!.options).map((o) => o.textContent);

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

const DOCKET = { harness_kind: 'docket' };

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

    btn(container, 'Export project file (YAML)').click();
    await vi.waitFor(() =>
      expect(
        fetchMock.mock.calls.some((c) => String(c[0]).includes('/api/projects/p1/export?format=yaml&scope=bundle')),
      ).toBe(true),
    );

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
    await vi.waitFor(() => expect(document.body.textContent).toContain('3 items will be added'));
    expect(document.body.textContent).toContain('Roles: 2 new, 5 updated, 3 not mentioned');
    expect(document.body.textContent).toContain('Planner is built in and was not changed');

    btn(document.body, 'Add items').click();
    await vi.waitFor(() =>
      expect(csvPosts()).toEqual(['/api/projects/p1/import-csv?dry_run=true', '/api/projects/p1/import-csv']),
    );
    dispose();
  });

  it('project_settings_follows_the_tab_in_the_address', async () => {
    let go!: (to: string) => void;
    const { container } = mount(() => {
      go = useNavigate();
      return <ProjectSettings />;
    });
    await flush();
    const active = () => container.querySelector('[role="tab"][aria-selected="true"]')?.textContent?.trim();
    expect(active()).toBe('General');
    go('/?tab=automation');
    await flush();
    expect(active()).toBe('Automation');
    go('/?tab=roles');
    await flush();
    expect(active()).toBe('Roles');
    go('/?tab=nonsense');
    await flush();
    expect(active()).toBe('General');
  });

  it('automation_panel_keeps_buttons_out_of_its_folded_headings', async () => {
    (PROJECT as { default_harness: string | null }).default_harness = 'claude-code';
    const { container } = mount(() => <AutomationPanel />);
    await flush();
    await flush();
    const summaries = Array.from(container.querySelectorAll('summary'));
    expect(summaries.length).toBeGreaterThan(0);
    for (const summary of summaries) {
      expect(summary.querySelector('button, a, input, select'), summary.textContent ?? '').toBeNull();
    }
    const how = Array.from(container.querySelectorAll('details')).find((d) =>
      d.querySelector('summary')?.textContent?.includes('How the agent works on it'),
    )!;
    expect(how.querySelector('p')?.textContent).toContain(HELP.howTheAgentWorks.text);
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
    const push = byText('label', 'Push the branch when a run finishes').querySelector('input')!;
    const install = container.querySelector<HTMLInputElement>('input[aria-label="Install command"]')!;
    const rows: [string, () => void, object][] = [
      ['folder', () => { folder.value = '/tmp/other'; folder.dispatchEvent(new Event('change', { bubbles: true })); }, { repository: '/tmp/other', default_branch: 'main' }],
      ['mode', () => byText('label', 'In the folder, directly').querySelector('input')!.click(), { workspace_mode: 'in_place' }],
      ['push', () => push.click(), { push_after_run: true }],
      ['install off', () => byText('label', 'Install dependencies before each run').querySelector('input')!.click(), { prepare_command: '' }],
      ['install command', () => { install.value = ' make deps '; install.dispatchEvent(new Event('change', { bubbles: true })); }, { prepare_command: 'make deps' }],
      ['finish', () => { select.value = 'Done'; select.dispatchEvent(new Event('change', { bubbles: true })); }, { on_finish_status: 'Done' }],
      ['harness', () => byText('button', 'Codex').click(), { default_harness: 'codex' }],
      ['profile', () => byText('button', 'Implementer').click(), { default_profile_id: 'prof1' }],
    ];
    const dod = container.querySelector<HTMLTextAreaElement>('textarea[aria-label="Definition of done"]')!;
    rows.push(
      ['listed model', () => chooseModel(container, 'claude-opus-5-5'), { default_model: 'claude-opus-5-5' }],
      ['agent default model', () => chooseModel(container, "Agent's default"), { default_model: null }],
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

  it('automation_panel_changing_the_default_agent_clears_its_default_model', async () => {
    const mutable = PROJECT as { default_harness: string | null; default_model: unknown };
    mutable.default_harness = 'claude-code';
    mutable.default_model = 'claude-opus-5-5';
    try {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      expect(container.querySelector<HTMLSelectElement>('select[aria-label="Model"]')!.value).toBe('claude-opus-5-5');
      btn(container, 'Codex').click();
      await flush();
      await flush();
      expect(lastModelPatch()).toEqual({ default_harness: 'codex', default_model: null });
    } finally {
      mutable.default_harness = null;
      mutable.default_model = undefined;
    }
  });

  it('automation_panel_says_whether_the_default_agent_runs_on_this_computer', async () => {
    const base = fetchMock.getMockImplementation()!;
    let docket: unknown = { ...DOCKET, sandbox: 'none', decisions: { support: 'supported' } };
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
      docket = { ...DOCKET, sandbox: 'none', decisions: { support: 'unsupported' } };
      const blocked = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      expect(line(blocked.container)).toContain("Doesn't run on this computer: it isn't sandboxed and can't ask before it acts");
    } finally {
      (PROJECT as { default_harness: string | null }).default_harness = null;
    }
  });

  it('automation_panel_lists_what_the_agent_reports_and_keeps_a_saved_unlisted_name_in_the_typed_field', async () => {
    const mutable = PROJECT as { default_harness: string | null; default_model: unknown };
    mutable.default_harness = 'claude-code';
    mutable.default_model = 'my-private-model';
    try {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      expect(modelLabels(container)).toEqual(["Agent's default", 'claude-sonnet-5-5', 'claude-opus-5-5', 'Another model…']);
      expect(container.querySelector<HTMLSelectElement>('select[aria-label="Model"]')!.selectedOptions[0]?.textContent).toBe('Another model…');
      expect(container.querySelector<HTMLInputElement>('input[aria-label="Model name"]')!.value).toBe('my-private-model');
    } finally {
      mutable.default_harness = null;
      mutable.default_model = undefined;
    }
  });

  it('automation_panel_saves_a_typed_default_for_an_agent_that_lists_no_models', async () => {
    (PROJECT as { default_harness: string | null }).default_harness = 'codex';
    try {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      expect(modelLabels(container)).toEqual(["Agent's default", 'Another model…']);
      expect(container.textContent).toContain("Codex can't list its models here. Type the model's name as Codex spells it.");
      chooseModel(container, 'Another model…');
      await flush();
      const typed = container.querySelector<HTMLInputElement>('input[aria-label="Model name"]')!;
      typed.value = ' vendor/Odd-Name ';
      typed.dispatchEvent(new Event('change', { bubbles: true }));
      await flush();
      await flush();
      expect(lastModelPatch()).toEqual({ default_model: 'vendor/Odd-Name' });
    } finally {
      (PROJECT as { default_harness: string | null }).default_harness = null;
    }
  });

  describe('The ladder', () => {
    const rung = (c: HTMLElement, name: string) => c.querySelector<HTMLElement>(`[data-testid="rung-${name}"]`)!;
    const ROLE = { id: 'role-1', project_id: 'p1', name: 'Content', color: '#fff', icon: null, created_at: '', agent_profile_id: null };

    it('renders_the_four_rungs_in_order', async () => {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const order = Array.from(container.querySelectorAll('[data-testid^="rung-"]')).map((e) => e.getAttribute('data-testid'));
      expect(order).toEqual(['rung-teams', 'rung-start', 'rung-merge', 'rung-ceiling']);
    });

    it('the_start_switch_is_disabled_with_its_reason_while_no_role_has_a_profile', async () => {
      roles = [ROLE];
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const start = rung(container, 'start');
      expect(start.querySelector<HTMLInputElement>('[role="switch"]')!.disabled).toBe(true);
      expect(start.textContent).toContain('Choose a profile for at least one role in Teams');
      expect(rung(container, 'teams').querySelector<HTMLButtonElement>('button[disabled]')!.textContent).toBe("Apply the repository's teams");
    });

    it('the_start_switch_works_once_a_role_has_a_profile', async () => {
      roles = [{ ...ROLE, agent_profile_id: 'prof1' }];
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const sw = rung(container, 'start').querySelector<HTMLInputElement>('[role="switch"]')!;
      expect(sw.disabled).toBe(false);
      sw.click();
      await flush();
      // The status the dropdown shows is saved with the switch, or the tick would skip the project.
      expect(lastModelPatch()).toEqual({ auto_start_enabled: true, auto_start_status: 'To Do' });
    });

    it('choosing_a_profile_for_a_role_patches_the_role', async () => {
      roles = [ROLE];
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const select = container.querySelector<HTMLSelectElement>('select[aria-label="Profile for Content"]')!;
      select.value = 'prof1';
      select.dispatchEvent(new Event('change', { bubbles: true }));
      await flush();
      const call = fetchMock.mock.calls.find((c) => String(c[0]).endsWith('/api/roles/role-1'))!;
      expect((call[1] as RequestInit).method).toBe('PATCH');
      expect(JSON.parse((call[1] as RequestInit).body as string)).toEqual({ agent_profile_id: 'prof1' });
    });

    it('a_team_row_says_what_the_server_says_and_the_apply_button_says_what_is_missing', async () => {
      roles = [{ ...ROLE, agent_profile_id: 'prof1' }];
      // Attended with no violation listed: the server still caps it, so the row says a person is needed.
      autonomy.teams = [{ name: 'saas-content', profile: 'default', caps_at: 'L3', fix: 'run it lights-out' }];
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const teams = rung(container, 'teams');
      expect(teams.querySelector('[data-testid="team-row"]')!.textContent).toContain('saas-content · needs a person · this computer');
      const apply = btn(teams, "Apply the repository's teams");
      expect(apply.disabled).toBe(true);
      const why = teams.querySelector('#' + CSS.escape(apply.getAttribute('aria-describedby')!))!;
      expect(why.textContent).toBe("Tack cannot run this on the team's computer from the board yet.");
      // No command and no variable on the screen: a link to the Administrator Guide instead.
      expect(teams.textContent).not.toMatch(/docket (init|setup)|TACK_/);
    });

    it('a_failed_load_says_so_with_a_retry_and_is_never_shown_as_empty', async () => {
      failing = ['/api/projects/p1/autonomy', '/api/projects/p1/roles', '/api/agent-profiles'];
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const alerts = Array.from(container.querySelectorAll('[data-testid="load-error"]')).map((a) => a.textContent);
      expect(alerts.some((a) => a!.startsWith('Could not load the autonomy settings.'))).toBe(true);
      expect(alerts.some((a) => a!.startsWith("Could not load this project's roles."))).toBe(true);
      expect(alerts.some((a) => a!.startsWith('Could not load the agent profiles'))).toBe(true);
      expect(rung(container, 'merge').textContent).toContain('The board is unreachable');
      expect(container.textContent).not.toContain('not available yet');
      expect(container.textContent).not.toContain('has no roles yet');

      // The board comes back: Try again loads what was missing.
      failing = [];
      btn(rung(container, 'merge'), 'Try again').click();
      await flush();
      await flush();
      expect(rung(container, 'merge').querySelector('[data-testid="load-error"]')).toBeNull();
      expect(rung(container, 'merge').textContent).toContain('You merge every change yourself.');
    });

    it('the_no_role_row_says_the_state_of_the_default_profiles_team', async () => {
      const mutable = PROJECT as { default_profile_id: string | null };
      mutable.default_profile_id = 'prof1';
      try {
        autonomy.teams = [{ name: 'saas-content', profile: 'default', caps_at: 'L3', fix: 'run it lights-out' }];
        const { container } = mount(() => <AutomationPanel />);
        await flush();
        await flush();
        expect(rung(container, 'teams').querySelector('[data-testid="team-row-default"]')!.textContent).toContain('saas-content · needs a person · this computer');
      } finally {
        mutable.default_profile_id = null;
      }
    });

    it('a_lights_out_team_row_says_lights_out', async () => {
      roles = [{ ...ROLE, agent_profile_id: 'prof1' }];
      autonomy.teams = [{ name: 'saas-content', profile: 'lights-out', caps_at: null, fix: null }];
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      expect(rung(container, 'teams').querySelector('[data-testid="team-row"]')!.textContent).toContain('saas-content · lights-out · this computer');
    });

    it('a_team_row_says_which_folders_its_sandbox_reads_and_what_to_do_when_there_are_none', async () => {
      roles = [{ ...ROLE, agent_profile_id: 'prof1' }];
      autonomy.teams = [{ name: 'saas-content', profile: 'lights-out', caps_at: null, fix: null, read_only: ['/home/x/.nvm/versions/node/v22.17.1', '/home/x/venv'] }];
      const { container, dispose } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const reads = () => Array.from(rung(container, 'teams').querySelectorAll('[data-testid="team-reads"]')).map((e) => e.textContent);
      expect(reads()[0]).toBe('Its sandbox also reads /home/x/.nvm/versions/node/v22.17.1, /home/x/venv, and cannot change them.');
      dispose();

      // Nothing granted: the row says so, and what an operator does about a team that runs tests.
      autonomy.teams = [{ name: 'saas-content', profile: 'lights-out', caps_at: null, fix: null, read_only: [] }];
      const none = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const text = rung(none.container, 'teams').querySelector('[data-testid="team-reads"]')!.textContent!;
      expect(text).toContain('reads only its own work');
      expect(text).toContain('needs its toolchain granted');
      none.dispose();

      // A docket that does not say adds no line, rather than claiming "nothing".
      autonomy.teams = [{ name: 'saas-content', profile: 'lights-out', caps_at: null, fix: null, read_only: null }];
      const unknown = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      expect(rung(unknown.container, 'teams').querySelector('[data-testid="team-reads"]')).toBeNull();
    });

    it('a_lights_out_team_line_renders_a_tick_in_the_ceiling', async () => {
      autonomy.teams = [{ name: 'saas-content', profile: 'Content', caps_at: null, fix: null }];
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const lines = Array.from(rung(container, 'ceiling').querySelectorAll('[data-testid="ceiling-team"]')).map((e) => e.textContent);
      expect(lines).toHaveLength(1);
      expect(lines[0]).toContain('✓');
      expect(lines[0]).not.toContain('✗');
    });

    it('a_team_line_with_a_fix_renders_in_the_ceiling', async () => {
      autonomy.ceiling = 'L3';
      autonomy.teams = [{ name: 'saas-content', profile: 'Content', caps_at: 'L3', fix: 'Turn on lights-out in the team file' }];
      autonomy.automatic_merge_blocked_by = [
        'The team saas-content runs attended: its tasks stay at L3. Fix: Turn on lights-out in the team file',
        'The project has no public key to check the verifier\'s signature against',
      ];
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      await flush();
      const ceiling = rung(container, 'ceiling');
      expect(ceiling.textContent).toContain('This project can reach L3.');
      const lines = Array.from(ceiling.querySelectorAll('[data-testid="ceiling-team"]')).map((e) => e.textContent);
      expect(lines).toHaveLength(1);
      expect(lines[0]).toContain('✗');
      expect(lines[0]).toContain('Turn on lights-out in the team file');
      expect(Array.from(ceiling.querySelectorAll('[data-testid="ceiling-blocker"]')).map((e) => e.textContent)).toEqual([
        "✗ The project has no public key to check the verifier's signature against",
      ]);
    });
  });

  describe('Autonomy', () => {
    it('the_merge_on_done_switch_saves', async () => {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      const sw = withInput(container, 'Merge when moved to Done');
      expect(sw.checked).toBe(false);
      sw.click();
      await flush();
      expect(autonomyPuts().at(-1)).toMatchObject({ merge_on_done: true, verify_loop_max_attempts: 2 });
    });

    it('the_autonomy_section_saves_each_field_and_fills_the_example_stages', async () => {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      const cap = withInput(container, 'Times a rejected change goes back to the agent');
      cap.value = '3';
      cap.dispatchEvent(new Event('change', { bubbles: true }));
      await flush();
      expect(autonomyPuts().at(-1)).toMatchObject({ verify_loop_max_attempts: 3 });

      const tokens = withInput(container, 'Tokens per day');
      tokens.value = '5000';
      tokens.dispatchEvent(new Event('change', { bubbles: true }));
      await flush();
      expect(autonomyPuts().at(-1).budgets).toMatchObject({ tokens_per_day: 5000 });

      btn(container, "Fill the example stages").click();
      await flush();
      expect(autonomyPuts().at(-1).verify_require).toEqual(['criteria', 'suite', 'anti_tautology', 'static_analysis']);
      expect(withInput(container, 'Stages the verifier must run').value).toBe('criteria, suite, anti_tautology, static_analysis');

      const key = container.querySelector<HTMLTextAreaElement>('textarea[aria-label="Public key"]')!;
      key.value = 'PEM';
      key.dispatchEvent(new Event('change', { bubbles: true }));
      await flush();
      expect(autonomyPuts().at(-1).attestation_public_key).toBe('PEM');
    });

    it('a_domain_shows_its_merged_and_undone_counts_before_its_level_control', async () => {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      const row = container.querySelector('[data-testid="autonomy-domain"]')!;
      expect(withInput(row as HTMLElement, 'Domain name').value).toBe('docs');
      const counts = row.querySelector('[data-testid="domain-counts"]')!;
      expect(counts.textContent).toContain('4 merged');
      expect(counts.textContent).toContain('1 undone');
      const level = row.querySelector('select')!;
      expect(counts.compareDocumentPosition(level) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      level.value = 'L5';
      level.dispatchEvent(new Event('change', { bubbles: true }));
      await flush();
      expect(autonomyPuts().at(-1).domains[0]).toMatchObject({ name: 'docs', level: 'L5', path_globs: ['docs/**'] });
    });

    it('a_disabled_L4_shows_its_reason', async () => {
      autonomy.automatic_merge_blocked_by = ['GitHub polling is off on this board, so Tack cannot read CI or merge'];
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      const l4 = container.querySelector<HTMLInputElement>('input[name="autonomy-level"][value="L4"]')!;
      const l5 = container.querySelector<HTMLInputElement>('input[name="autonomy-level"][value="L5"]')!;
      const l3 = container.querySelector<HTMLInputElement>('input[name="autonomy-level"][value="L3"]')!;
      expect(l4.disabled).toBe(true);
      expect(l5.disabled).toBe(true);
      expect(l3.disabled).toBe(false);
      expect(container.querySelector('[data-testid="autonomy-blocked"]')!.textContent).toContain('GitHub polling is off');
    });

    it('an_enabled_L4_saves_the_level', async () => {
      const { container } = mount(() => <AutomationPanel />);
      await flush();
      const l4 = container.querySelector<HTMLInputElement>('input[name="autonomy-level"][value="L4"]')!;
      expect(l4.disabled).toBe(false);
      l4.click();
      await flush();
      expect(autonomyPuts().at(-1).level).toBe('L4');
    });
  });
});
