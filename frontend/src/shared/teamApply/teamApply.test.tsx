import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { CreateTeam, createTeamApply } from './teamApply';
import { flush } from '../../test/flush';

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

const PROJECT = '11111111-1111-1111-1111-111111111111';

const runnerWith = (apiFeatures: string[]) => ({
  runner_id: 'r1', name: 'This computer', state: 'active', online: true, labels: null, labels_raw: '{}', total_capacity: 1, available_capacity: 1,
  capability_snapshot: {
    harnesses: [{ harness_kind: 'docket', installed_version: '', api_version: '6', probe_error: null, probed_at: '', api_features: apiFeatures }],
    concurrency: { max_concurrent: 1 }, limits: {},
  },
  capability_snapshot_raw: '', protocol_version: 1, runner_version: '1', last_heartbeat_at: null, revoked_at: null, fleet_ids: [], created_at: '', updated_at: '',
});

type Applied = { teams: Array<{ name: string; shape: string; accept?: string }> };

/** The board's answers: `runners` for GET /runners, `answers` (one per call) for POST /teams/apply. */
function mockBoard(apiFeatures: string[], answers: unknown[] = []) {
  const bodies: Applied[] = [];
  const resolvers: Array<() => void> = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
    const url = String(input);
    if (url.endsWith('/api/runners')) return new Response(JSON.stringify({ protocol_version: 1, data: [runnerWith(apiFeatures)] }), { status: 200 });
    if (url.endsWith(`/api/projects/${PROJECT}/teams/apply`)) {
      bodies.push(JSON.parse(String(init?.body)));
      const next = answers.shift();
      if (next === 'hold') await new Promise<void>((resolve) => resolvers.push(resolve));
      const answer = next === 'hold' ? answers.shift() : next;
      if (answer && typeof answer === 'object' && 'error' in answer) return new Response(JSON.stringify(answer), { status: 409 });
      return new Response(JSON.stringify(answer ?? []), { status: 200 });
    }
    return new Response('{}', { status: 200 });
  });
  return { bodies, release: () => resolvers.shift()?.() };
}

function mount(options: { projectId?: string; shape?: string; onApplied?: () => void } = {}) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const projectId = 'projectId' in options ? options.projectId : PROJECT;
  render(() => {
    const teams = createTeamApply({ projectId: () => projectId, onApplied: options.onApplied });
    return (
      <>
        <CreateTeam name="saas-ops" shape={options.shape} teams={teams} />
        <teams.Host />
      </>
    );
  }, container);
  return container;
}

const create = (c: HTMLElement) => c.querySelector<HTMLButtonElement>('button[aria-label="Create the team saas-ops"]')!;
const settle = async () => { await flush(); await flush(); await flush(); };

describe('Create team', () => {
  it('is enabled when a computer lists podShape, and disabled with the reason when none does', async () => {
    mockBoard(['podShape']);
    const on = mount();
    await settle();
    expect(create(on).disabled).toBe(false);
    document.body.innerHTML = '';

    vi.restoreAllMocks();
    mockBoard(['podResume']);
    const off = mount();
    await settle();
    expect(create(off).disabled).toBe(true);
    expect(off.textContent).toContain('does not list podShape');
  });

  it('is disabled with the reason when there is no project', async () => {
    mockBoard(['podShape']);
    const c = mount({ projectId: undefined });
    await settle();
    expect(create(c).disabled).toBe(true);
    expect(c.textContent).toContain('Choose a project');
  });

  it('sends the shape the file gave, once, and refreshes the screen when the team is made', async () => {
    const board = mockBoard(['podShape'], [[{ team: 'saas-ops', state: 'applied' }]]);
    const onApplied = vi.fn();
    const c = mount({ shape: 'respond', onApplied });
    await settle();
    create(c).click();
    await settle();
    expect(board.bodies).toEqual([{ teams: [{ name: 'saas-ops', shape: 'respond' }] }]);
    expect(onApplied).toHaveBeenCalledOnce();
    expect(c.querySelector('[data-testid="team-apply-notes"]')!.textContent).toContain('saas-ops is made');
  });

  it('asks for the shape when no file gave it', async () => {
    const board = mockBoard(['podShape'], [[{ team: 'saas-ops', state: 'applied' }]]);
    const c = mount();
    await settle();
    const select = c.querySelector<HTMLSelectElement>('select[aria-label="Shape of saas-ops"]')!;
    expect(select.value).toBe('build');
    select.value = 'operate';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    create(c).click();
    await settle();
    expect(board.bodies[0].teams[0].shape).toBe('operate');
  });

  it('shows the sentence a refusal carries, and the one a whole refused call carries', async () => {
    mockBoard(['podShape'], [
      [{ team: 'saas-ops', state: 'refused', message: 'The folder .docket/teams/saas-ops does not exist.' }],
      { error: { code: 'conflict', message: 'No computer reports a docket that lists podShape. Update it.' } },
    ]);
    const c = mount({ shape: 'build' });
    await settle();
    create(c).click();
    await settle();
    expect(c.querySelector('[data-testid="team-apply-notes"] [data-state="refused"]')!.textContent).toBe('saas-ops: The folder .docket/teams/saas-ops does not exist.');
    create(c).click();
    await settle();
    expect(c.querySelector('[data-testid="team-apply-notes"]')!.textContent).toBe('saas-ops: No computer reports a docket that lists podShape. Update it.');
  });

  it('is busy while docket works and does not send a second call', async () => {
    const board = mockBoard(['podShape'], ['hold', [{ team: 'saas-ops', state: 'applied' }]]);
    const c = mount({ shape: 'build' });
    await settle();
    create(c).click();
    create(c).click();
    await settle();
    expect(create(c).getAttribute('aria-busy')).toBe('true');
    expect(board.bodies).toHaveLength(1);
    board.release();
    await settle();
    expect(create(c).getAttribute('aria-busy')).toBeNull();
    expect(board.bodies).toHaveLength(1);
  });

  it('lists the commands docket wants read, and sends the team again with its code once they are accepted', async () => {
    const board = mockBoard(['podShape'], [
      [{ team: 'saas-ops', state: 'needs_review', review: { code: 'rv-7', commands: ['npm ci', 'make seed'] } }],
      [{ team: 'saas-ops', state: 'applied' }],
    ]);
    const onApplied = vi.fn();
    const c = mount({ shape: 'operate', onApplied });
    await settle();
    create(c).click();
    await settle();
    const dialog = document.querySelector('[data-testid="team-apply-review"]')!;
    expect(dialog.textContent).toContain('.docket/teams/saas-ops');
    expect(Array.from(dialog.querySelectorAll('li')).map((li) => li.textContent)).toEqual(['npm ci', 'make seed']);
    expect(onApplied).not.toHaveBeenCalled();
    Array.from(dialog.querySelectorAll('button')).find((b) => b.textContent === 'Accept and apply')!.click();
    await settle();
    expect(board.bodies).toEqual([
      { teams: [{ name: 'saas-ops', shape: 'operate' }] },
      { teams: [{ name: 'saas-ops', shape: 'operate', accept: 'rv-7' }] },
    ]);
    expect(document.querySelector('[data-testid="team-apply-review"]')).toBeNull();
    expect(onApplied).toHaveBeenCalledOnce();
  });

  it('makes nothing when the commands are not accepted', async () => {
    const board = mockBoard(['podShape'], [[{ team: 'saas-ops', state: 'needs_review', review: { code: 'rv-7', commands: ['make seed'] } }]]);
    const c = mount({ shape: 'operate' });
    await settle();
    create(c).click();
    await settle();
    Array.from(document.querySelectorAll('[data-testid="team-apply-review"] button')).find((b) => b.textContent === 'Cancel')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await settle();
    expect(board.bodies).toHaveLength(1);
    expect(c.querySelector('[data-testid="team-apply-notes"]')!.textContent).toContain('saas-ops is not made');
  });
});
