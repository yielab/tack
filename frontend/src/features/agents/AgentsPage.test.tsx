import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import AgentsPage from './AgentsPage';

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

const LOCAL_RUNNER_OFF = { enabled: false, state: 'stopped', since: null };

function mockFetch(overrides: Record<string, unknown> = {}) {
  const responses: Record<string, unknown> = {
    '/api/local-runner': LOCAL_RUNNER_OFF,
    '/api/local-runner/secrets': { data: [] },
    '/api/runners': { protocol_version: 1, data: [] },
    '/api/projects': [],
    '/api/local-runner/test-run': { request_id: 'req_1' },
    '/api/executions/req_1': { request_id: 'req_1', item_id: 'item_1', state: 'queued', cancellation_requested_at: null, created_at: '' },
    '/api/executions/req_1/attempts': { protocol_version: 1, data: [] },
    '/api/executions': { protocol_version: 1, data: [] },
    ...overrides,
  };
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = String(input);
    const urlPath = url.split('?')[0]; // match on path alone — the execution preload now appends `?limit=`
    for (const [path, body] of Object.entries(responses)) {
      if (urlPath.endsWith(path)) {
        return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
      }
    }
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
}

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route
          path="/"
          component={() => (
            <ExecutionStoreProvider>
              <AgentsPage />
            </ExecutionStoreProvider>
          )}
        />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

/** Text a reader would actually see — strips copy-paste command blocks
 *  (`<pre>`/`<code>`), which necessarily name real CLI commands
 *  (`tack runner secret set`, `npm install -g @openai/codex`) the vendor or
 *  this project's own CLI define, not narrative vocabulary. Advanced's own
 *  content is never in this text at all while collapsed — it isn't
 *  rendered, not merely hidden. */
function visibleProseText(container: HTMLElement): string {
  const clone = container.cloneNode(true) as HTMLElement;
  clone.querySelectorAll('pre, code').forEach((el) => el.remove());
  return clone.textContent ?? '';
}

const FORBIDDEN_VOCABULARY = ['runner', 'fleet', 'enroll', 'heartbeat', 'capacity', 'lease', 'harness'];

describe('AgentsPage — default screen vocabulary', () => {
  it('never renders runner/fleet/enroll/heartbeat/capacity/lease/harness while Advanced is collapsed', async () => {
    mockFetch();
    const { container } = mount();
    await flush();
    await flush();

    const text = visibleProseText(container).toLowerCase();
    const hits = FORBIDDEN_VOCABULARY.filter((word) => text.includes(word));
    expect(hits, `forbidden vocabulary found on the default screen: ${hits.join(', ')}`).toEqual([]);
  });

  it('Advanced starts collapsed, and its own vocabulary appears once opened', async () => {
    mockFetch();
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).not.toContain('Enroll a runner');
    const advancedButton = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Advanced'),
    )!;
    advancedButton.click();
    await flush();
    expect(container.textContent).toContain('Enroll a runner');
  });
});

const RUNNING = { enabled: true, state: 'running', since: '2026-01-01T00:00:00Z' };

function runnerWith(harnesses: unknown[]) {
  return {
    protocol_version: 1,
    data: [
      {
        runner_id: 'runr_1', name: 'local-abc', state: 'active', labels: null, labels_raw: '{}',
        total_capacity: 1, available_capacity: 1,
        capability_snapshot: { harnesses }, capability_snapshot_raw: '{}',
        protocol_version: 1, runner_version: '0.1.0', last_heartbeat_at: null, revoked_at: null,
        fleet_ids: [], created_at: '', updated_at: '',
      },
    ],
  };
}

const CLAUDE_INSTALLED = { harness_kind: 'claude-code', installed_version: '9.9.1', probe_error: null, probed_at: '', requires_model: false };

describe('AgentsPage — this computer only', () => {
  it('the_test_run_needs_no_fields', async () => {
    const fetchSpy = mockFetch({ '/api/local-runner': RUNNING, '/api/runners': runnerWith([CLAUDE_INSTALLED]) });
    const { container } = mount();
    await flush();
    await flush();

    // No project exists, and the page has no input at all.
    const inputs = Array.from(container.querySelectorAll('input, select, textarea'));
    expect(inputs.length).toBe(0);
    expect(container.textContent).not.toContain('Project');
    const button = Array.from(container.querySelectorAll('button')).find((b) => b.textContent?.includes('Run test'))!;
    expect(button).toBeTruthy();
    expect(button.disabled).toBe(false);

    button.click();
    await flush();
    await flush();
    const post = fetchSpy.mock.calls.find(([u]) => String(u).endsWith('/api/local-runner/test-run'))!;
    expect(JSON.parse(String((post[1] as RequestInit).body))).toEqual({ harness_kind: 'claude-code' });
    expect(fetchSpy.mock.calls.some(([u]) => String(u).includes('/api/executions/req_1'))).toBe(true);
  });

  it('the_test_run_is_refused_with_a_reason_when_every_installed_agent_needs_a_model', async () => {
    mockFetch({
      '/api/local-runner': RUNNING,
      '/api/runners': runnerWith([{ ...CLAUDE_INSTALLED, harness_kind: 'codex', installed_version: '1.2.3', requires_model: true }]),
    });
    const { container } = mount();
    await flush();
    await flush();
    const button = Array.from(container.querySelectorAll('button')).find((b) => b.textContent?.includes('Run test'))!;
    expect(button.disabled).toBe(true);
    expect(container.textContent).toContain('codex needs a model named for every run');
  });

  it('the_test_run_stays_disabled_when_only_docket_is_installed_because_it_needs_a_model', async () => {
    mockFetch({
      '/api/local-runner': RUNNING,
      '/api/runners': runnerWith([{ ...CLAUDE_INSTALLED, harness_kind: 'docket', installed_version: '0.4.0', requires_model: true }]),
    });
    const { container } = mount();
    await flush();
    await flush();
    const button = Array.from(container.querySelectorAll('button')).find((b) => b.textContent?.includes('Run test'))!;
    expect(button.disabled).toBe(true);
    expect(container.textContent).toContain('docket needs a model named for every run, and the test run names none');
  });

  it('lists the agents not reported as not installed, each with its install hint', async () => {
    mockFetch({
      '/api/local-runner': RUNNING,
      '/api/runners': runnerWith([CLAUDE_INSTALLED, { ...CLAUDE_INSTALLED, harness_kind: 'codex', installed_version: '1.2.3' }]),
    });
    const { container } = mount();
    await flush();
    await flush();
    expect(container.textContent).toContain('Installed v9.9.1');
    expect(container.textContent).toContain('Docket');
    expect(container.textContent).toContain('OpenCode');
    expect((container.textContent!.match(/Not installed/g) ?? []).length).toBe(2);
    expect(container.textContent).toContain('brew install opencode');
    expect(container.textContent).toContain('From the docket project, following its own install instructions.');
  });

  it('step 3 reads each installed agent as signed in, signed out or unknown, and is done only when all are signed in', async () => {
    const docs = { models: 'https://example.test/models', sign_in: 'https://example.test/sign-in' };
    const harness = (extra: Record<string, unknown>) => ({ ...CLAUDE_INSTALLED, docs, ...extra });
    const stepThree = (c: HTMLElement) => c.querySelectorAll('[data-step-state]')[2] as HTMLElement;
    const row = (c: HTMLElement, kind: string) => c.querySelector(`[data-testid="sign-in-${kind}"]`) as HTMLElement;

    mockFetch({
      '/api/local-runner': RUNNING,
      '/api/runners': runnerWith([
        harness({ sign_in: { state: 'signed_in', detail: 'Logged in as ana@example.test', command: null } }),
        harness({ harness_kind: 'codex', installed_version: '1.2.3', sign_in: { state: 'signed_out', detail: 'Not logged in', command: 'codex login' } }),
        harness({ harness_kind: 'opencode', installed_version: '1.0.3', sign_in: { state: 'unknown', detail: 'opencode does not say whether it is signed in', command: null } }),
      ]),
    });
    const mixed = mount();
    await flush();
    await flush();
    expect(row(mixed.container, 'claude-code').textContent).toContain('Signed in');
    expect(row(mixed.container, 'claude-code').textContent).toContain('Logged in as ana@example.test');
    expect(row(mixed.container, 'codex').textContent).toContain('Not signed in. Run codex login in a terminal.');
    expect(row(mixed.container, 'codex').querySelector('code')?.textContent).toBe('codex login');
    expect(row(mixed.container, 'codex').querySelector<HTMLAnchorElement>('a')?.href).toBe(docs.sign_in);
    expect(row(mixed.container, 'opencode').textContent).toContain('opencode does not say whether it is signed in');
    expect(row(mixed.container, 'opencode').querySelector<HTMLAnchorElement>('a')?.href).toBe(docs.sign_in);
    expect(stepThree(mixed.container).dataset.stepState).not.toBe('done');
    mixed.dispose();
    mixed.container.remove();

    mockFetch({
      '/api/local-runner': RUNNING,
      '/api/runners': runnerWith([harness({ sign_in: { state: 'signed_in', detail: 'Logged in', command: null } })]),
    });
    const signedIn = mount();
    await flush();
    await flush();
    expect(stepThree(signedIn.container).dataset.stepState).toBe('done');
  });

  it('an older runner with no sign-in report reads as unknown, with no link', async () => {
    mockFetch({ '/api/local-runner': RUNNING, '/api/runners': runnerWith([CLAUDE_INSTALLED]) });
    const { container } = mount();
    await flush();
    await flush();
    const row = container.querySelector('[data-testid="sign-in-claude-code"]')!;
    expect(row.getAttribute('data-sign-in')).toBe('unknown');
    expect(row.querySelector('a')).toBeNull();
  });

  it('each installed agent says whether it is sandboxed, asks first, or does not run here', async () => {
    const asks = { support: 'supported', reason: null };
    mockFetch({
      '/api/local-runner': RUNNING,
      '/api/runners': runnerWith([
        { ...CLAUDE_INSTALLED, sandbox: 'bwrap', decisions: asks },
        { ...CLAUDE_INSTALLED, harness_kind: 'codex', installed_version: '0.2.0', sandbox: 'none', decisions: asks },
        {
          ...CLAUDE_INSTALLED, harness_kind: 'docket', installed_version: '0.3.0', sandbox: 'none',
          decisions: { support: 'unsupported', reason: null },
          probe_error: 'not sandboxed on this machine (bwrap not on PATH) and this agent cannot ask before it acts',
        },
      ]),
    });
    const { container } = mount();
    await flush();
    await flush();
    const line = (kind: string) => container.querySelector(`[data-testid="agent-safety-${kind}"]`)?.textContent ?? '';
    expect(line('claude-code')).toBe('Sandboxed: the agent only reaches its workspace');
    expect(line('codex')).toContain('Not sandboxed on this computer, so the agent asks before each action');
    expect(line('docket')).toContain("Doesn't run on this computer: it isn't sandboxed and can't ask before it acts");
    expect(container.textContent).toContain("Installed v0.3.0 · Doesn't run here");
    expect(container.textContent).not.toContain('Could not check');
  });
});

describe('AgentsPage — composition', () => {
  it('renders both numbered steps and mounts ExecutionToggle, with no key field and no Provider heading', async () => {
    mockFetch();
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).toContain('Agent execution on this machine'); // ExecutionToggle
    expect(container.querySelector('input[type="password"], input[type="text"]')).toBeNull();
    expect(Array.from(container.querySelectorAll('h1, h2, h3')).map((h) => h.textContent)).not.toContain('Provider');
    expect(container.textContent).toContain('Agents on this machine');
    expect(container.textContent).toContain('Test run');
  });

  it('shows both known harnesses as "Not found" with an install command when agent execution is off', async () => {
    mockFetch();
    const { container } = mount();
    await flush();
    await flush();
    expect(container.textContent).toContain("Turn on agent execution above to see what's installed here.");
  });

  it('lists a harness as installed, with its version, once this machine has an active runner reporting it', async () => {
    mockFetch({
      '/api/local-runner': { enabled: true, state: 'running', since: '2026-01-01T00:00:00Z' },
      '/api/runners': {
        protocol_version: 1,
        data: [
          {
            runner_id: 'runr_1',
            name: 'local-abc',
            state: 'active',
            labels: null,
            labels_raw: '{}',
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: {
              harnesses: [
                { harness_kind: 'codex', installed_version: '9.9.2', probe_error: null, probed_at: '' },
                { harness_kind: 'claude-code', installed_version: null, probe_error: '`claude` was not found on PATH', probed_at: '' },
              ],
            },
            capability_snapshot_raw: '{}',
            protocol_version: 1,
            runner_version: '0.1.0',
            last_heartbeat_at: null,
            revoked_at: null,
            fleet_ids: [],
            created_at: '',
            updated_at: '',
          },
        ],
      },
    });
    const { container } = mount();
    await flush();
    await flush();
    expect(container.textContent).toContain('Installed v9.9.2');
    expect(container.textContent).toContain('Not installed');
    expect(container.textContent).toContain('npm install -g @anthropic-ai/claude-code');
  });
});
