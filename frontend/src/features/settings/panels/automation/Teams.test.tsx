import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import Teams from './Teams';
import type { Project, Role } from '../../../../shared/types';
import { flush } from '../../../../test/flush';

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

const PROJECT_ID = '11111111-1111-1111-1111-111111111111';

const runner = (apiFeatures: string[]) => ({
  runner_id: 'r1', name: 'This computer', state: 'active', online: true, labels: null, labels_raw: '{}', total_capacity: 1, available_capacity: 1,
  capability_snapshot: {
    harnesses: [{ harness_kind: 'docket', installed_version: '', api_version: '6', probe_error: null, probed_at: '', api_features: apiFeatures }],
    concurrency: { max_concurrent: 1 }, limits: {},
  },
  capability_snapshot_raw: '', protocol_version: 1, runner_version: '1', last_heartbeat_at: null, revoked_at: null, fleet_ids: [], created_at: '', updated_at: '',
});

const PROFILES = [
  { agent_profile_id: 'ap_ops', name: 'Operator', tool_policy: { agent: 'docket', docket: { team: 'fab-ops' } } },
  { agent_profile_id: 'ap_code', name: 'Coder', tool_policy: { agent: 'docket', docket: { team: 'fab-code' } } },
];

const code = { name: 'fab-code', harness: 'docket', reported: true, runner: 'This computer', profiles: [], report: { name: 'fab-code' } };

function mount(options: { apiFeatures: string[]; teams?: unknown[]; project?: boolean; answers?: unknown[] }) {
  const bodies: unknown[] = [];
  const answers = options.answers ?? [];
  vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
    const url = String(input);
    if (url.endsWith('/api/runners')) return new Response(JSON.stringify({ protocol_version: 1, data: [runner(options.apiFeatures)] }), { status: 200 });
    if (url.endsWith('/api/teams')) return new Response(JSON.stringify({ teams: options.teams ?? [code] }), { status: 200 });
    if (url.endsWith('/teams/apply')) {
      bodies.push(JSON.parse(String(init?.body)));
      return new Response(JSON.stringify(answers.shift() ?? []), { status: 200 });
    }
    return new Response(JSON.stringify({ data: [] }), { status: 200 });
  });
  const roles = [{ id: 'r_ops', name: 'Operations', agent_profile_id: 'ap_ops' }] as unknown as Role[];
  const project = { id: PROJECT_ID, name: 'Shop', default_profile_id: 'ap_code' } as unknown as Project;
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(
    () => (
      <Teams
        roles={() => roles}
        profiles={() => PROFILES as never}
        settings={() => null}
        rolesError={() => undefined}
        onRetryRoles={() => {}}
        project={() => (options.project === false ? undefined : project)}
        onRoleSaved={() => {}}
        onDefaultProfile={() => {}}
      />
    ),
    container,
  );
  return { container, bodies };
}

const button = (c: HTMLElement) => Array.from(c.querySelectorAll('button')).find((b) => b.textContent === "Apply the repository's teams")!;
const settle = async () => { await flush(); await flush(); await flush(); };

describe("Apply the repository's teams", () => {
  it('is enabled when a computer lists podShape and a team the roles use is missing, and sends that team', async () => {
    const { container, bodies } = mount({ apiFeatures: ['podShape'], answers: [[{ team: 'fab-ops', state: 'applied' }]] });
    await settle();
    expect(button(container).disabled).toBe(false);
    expect(Array.from(container.querySelectorAll('[data-testid="missing-teams"] li')).map((li) => li.textContent)).toEqual(['fab-opsbuildrespondoperateroutine']);
    button(container).click();
    await settle();
    expect(bodies).toEqual([{ teams: [{ name: 'fab-ops', shape: 'build' }] }]);
    expect(container.querySelector('[data-testid="team-apply-notes"]')!.textContent).toContain('fab-ops is made');
  });

  it('is disabled with the reason when docket does not list podShape', async () => {
    const { container } = mount({ apiFeatures: [] });
    await settle();
    expect(button(container).disabled).toBe(true);
    expect(container.textContent).toContain('does not list podShape');
  });

  it('is disabled with the reason when every team the roles use is reported', async () => {
    const { container } = mount({ apiFeatures: ['podShape'], teams: [code, { ...code, name: 'fab-ops' }] });
    await settle();
    expect(button(container).disabled).toBe(true);
    expect(container.textContent).toContain('already reported');
  });

  it('is disabled with the reason when there is no project', async () => {
    const { container } = mount({ apiFeatures: ['podShape'], project: false });
    await settle();
    expect(button(container).disabled).toBe(true);
    expect(container.textContent).toContain('Choose a project');
  });
});
