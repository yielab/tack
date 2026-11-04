import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { createSignal } from 'solid-js';
import { MemoryRouter, Route } from '@solidjs/router';
import { ExecutionStoreProvider } from '../state/executionContext';
import RunWithAgentModal from './RunWithAgentModal';
import type { RunnerCapabilities } from '../execution';

// The real timer, taken before any test fakes `setTimeout`, so a flush still waits for the event loop.
const realSetTimeout = globalThis.setTimeout;
const flush = () => new Promise((r) => realSetTimeout(r, 0));
const disposers: Array<() => void> = [];

/** The profile's radio in the "Who does it" picker, found by the label a person reads. */
const profileRadio = (name: string): HTMLInputElement =>
  [...document.querySelectorAll<HTMLLabelElement>('[data-testid="who-does-it"] label')]
    .filter((l) => l.textContent === name)
    .map((l) => document.getElementById(l.htmlFor) as HTMLInputElement)[0];

/** The agent choice above the profiles. */
const agentButton = (kind: string) =>
  document.querySelector<HTMLButtonElement>(`[data-testid="who-agent-choice"] [data-agent="${kind}"]`)!;
const chosenAgent = () => document.querySelector('[data-testid="who-agent-choice"] [aria-checked="true"]')?.getAttribute('data-agent');
const agentChoices = () => [...document.querySelectorAll('[data-testid="who-agent-choice"] [data-agent]')].map((b) => [b.getAttribute('data-agent'), b.textContent]);

const FLEET = { fleet_id: 'fleet-1', name: 'Primary Fleet', concurrency_limit: 5, default_policy: null };
const PROFILE = {
  agent_profile_id: 'profile-1',
  name: 'Reviewer',
  instructions: 'Review the diff',
  tool_policy: { read: true },
  limits: null,
};
/** A second profile — with only one candidate, `RunWithAgentModal` auto-selects
 *  it (the same "an unambiguous single choice needs no picker" reasoning the
 *  target picker applies), so any test proving "no agent profile selected"
 *  needs at least two to keep that a real, unresolved choice. */
const PROFILE_2 = { agent_profile_id: 'profile-2', name: 'Fixer', instructions: 'Fix the bug', tool_policy: {}, limits: null };

/** One active runner reporting `codex`, which does not say whether it needs a model —
 *  the common shape most tests start from. */
function runnerCapabilitySnapshot(overrides: Record<string, unknown> = {}) {
  return {
    harnesses: [
      {
        harness_kind: 'codex',
        installed_version: '1.0.0',
        probe_error: null,
        probed_at: '2026-08-06T12:00:00Z',
      },
    ],
    concurrency: { total: 1, available: 1 },
    limits: { event_payload_bytes_max: 1024, artifact_content_bytes_max: 1024 },
    features: {
      cancel: { support: 'advisory', reason: null },
      resume: { support: 'unsupported', reason: null },
      decisions: { support: 'supported', reason: null },
      artifacts: { support: 'supported', reason: null },
      usage: { support: 'advisory', reason: null },
    },
    ...overrides,
  };
}

function runnerRow(id: string, name: string, snapshot: Record<string, unknown>, fleetIds: string[] = []) {
  return {
    runner_id: id,
    name,
    state: 'active',
    labels: null,
    labels_raw: '{}',
    total_capacity: 1,
    available_capacity: 1,
    capability_snapshot: snapshot,
    capability_snapshot_raw: JSON.stringify(snapshot),
    protocol_version: 1,
    runner_version: '0.1.0',
    last_heartbeat_at: '2026-08-06T12:00:00Z',
    revoked_at: null,
    fleet_ids: fleetIds,
    created_at: '2026-08-06T12:00:00Z',
    updated_at: '2026-08-06T12:00:00Z',
  };
}

const RUNNER = runnerRow('runner-1', 'Dev laptop', runnerCapabilitySnapshot());

/** A project that has a folder: the dialog needs nothing from the operator. */
const PROJECT = {
  id: 'project-1', name: 'P', default_model: null, code_origin: 'folder', repository: '/home/alice/src/app',
  default_branch: 'main', workspace_mode: 'local_branch', default_profile_id: 'profile-1',
};

let lastCreateBody: unknown;
let lastItemPatch: unknown;
let requestedUrls: string[] = [];

function mockFetch(
  opts: {
    runners?: unknown[];
    fleets?: unknown[];
    agentProfiles?: unknown[];
    project?: unknown;
    agentContext?: string;
    /** The resolved profile's `limits`, as the agent-context route returns them. */
    limits?: Record<string, unknown>;
    /** What the server resolved for the item: the profile and where it came from. */
    resolved?: { profile_id: string | null; profile_source: string | null };
    runSettings?: Record<string, unknown> | null;
    /** Holds the item back, so the runners land before the project's settings are applied. */
    itemDelayMs?: number;
    /** The item's brief; absent answers 404, as an item with no brief does. */
    brief?: Record<string, unknown>;
    /** While `down` is true the profiles list answers 500. */
    profilesFailing?: { down: boolean };
    /** The project's autonomy settings: each team's line, with `works_in`. */
    autonomy?: { teams: unknown[] };
  } = {},
): typeof fetch {
  const runners = opts.runners ?? [RUNNER];
  const fleets = opts.fleets ?? [FLEET];
  const agentProfiles: unknown[] = [...(opts.agentProfiles ?? [PROFILE])];
  const project = opts.project ?? PROJECT;
  return (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    requestedUrls.push(url);
    if (url.includes('/runner-fleets')) return new Response(JSON.stringify({ protocol_version: 1, data: fleets }), { status: 200 });
    if (url.includes('/agent-profiles') && init?.method === 'POST') {
      const body = JSON.parse(String(init.body));
      const created = { agent_profile_id: 'profile-new', name: body.name, instructions: body.instructions, tool_policy: body.tool_policy ?? {}, limits: null };
      agentProfiles.push(created);
      return new Response(JSON.stringify({ protocol_version: 1, ...created }), { status: 200 });
    }
    if (url.includes('/agent-profiles') && opts.profilesFailing?.down) return new Response(JSON.stringify({ error: { status: 500, message: 'boom' } }), { status: 500 });
    if (url.includes('/agent-profiles')) return new Response(JSON.stringify({ protocol_version: 1, data: agentProfiles }), { status: 200 });
    if (url.includes('/runners')) return new Response(JSON.stringify({ protocol_version: 1, data: runners }), { status: 200 });
    if (url.endsWith('/items/item-1/brief')) {
      return opts.brief
        ? new Response(JSON.stringify(opts.brief), { status: 200 })
        : new Response(JSON.stringify({ error: { status: 404, message: 'Item item-1 has no brief' } }), { status: 404 });
    }
    if (url.endsWith('/items/item-1')) {
      if (opts.itemDelayMs) await new Promise((r) => setTimeout(r, opts.itemDelayMs));
      if (init?.method === 'PATCH') lastItemPatch = JSON.parse(String(init.body));
      return new Response(JSON.stringify({ item: { id: 'item-1', run_settings: opts.runSettings ?? null }, id: 'item-1', run_settings: opts.runSettings ?? null }), { status: 200, headers: { ETag: '"1"' } });
    }
    if (url.includes('/agent-context')) {
      // As the server resolves it: the project's default, else the only profile there is.
      const fallback = (project as { default_profile_id?: string | null }).default_profile_id
        ?? (agentProfiles.length === 1 ? (agentProfiles[0] as { agent_profile_id: string }).agent_profile_id : null);
      const resolved = opts.resolved ?? { profile_id: fallback, profile_source: fallback ? 'project_default' : null };
      return new Response(JSON.stringify({ text: opts.agentContext ?? 'Fix login bug\n\nThe brief.', limits: opts.limits ?? {}, limit: { tokens: null, source: 'none', enforced_by: 'measured', sentence: `Server says ${new URL(url, 'http://x').search}` }, ...resolved }), { status: 200 });
    }
    if (url.endsWith('/autonomy')) return new Response(JSON.stringify(opts.autonomy ?? {}), { status: 200 });
    if (url.includes('/projects/')) return new Response(JSON.stringify(project), { status: 200 });
    if (url.endsWith('/executions') && init?.method === 'POST') {
      lastCreateBody = JSON.parse(String(init.body));
      return new Response(JSON.stringify({ protocol_version: 1, request_id: 'req-9', state: 'queued', replayed: false }), { status: 200 });
    }
    if (url.includes('/executions/req-9')) {
      return new Response(
        JSON.stringify({ request_id: 'req-9', item_id: 'item-1', state: 'queued', cancellation_requested_at: null, created_at: '2025-01-01T00:00:00Z' }),
        { status: 200 },
      );
    }
    if (url.includes('/executions')) return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    return new Response(JSON.stringify({}), { status: 200 });
  }) as typeof fetch;
}

function Host(props: { capabilities?: () => RunnerCapabilities[]; onCreated?: (id: string) => void; autonomy?: 'L3'; fromRun?: string }) {
  const [open, setOpen] = createSignal(true);
  (Host as unknown as { lastSetOpen?: (v: boolean) => void }).lastSetOpen = setOpen;
  return (
    <ExecutionStoreProvider>
      {/* `<A href="/agents">` (the "agent execution is off" state) needs a
       *  router context to resolve its path — every real mount point
       *  (Board/item-detail/Sprint) already renders under `app/routes.tsx`. */}
      <MemoryRouter>
        <Route
          path="/"
          component={() => (
            <RunWithAgentModal
              isOpen={open()}
              onClose={() => setOpen(false)}
              itemId="item-1"
              itemTitle="Fix login bug"
              projectId="project-1"
              onCreated={props.onCreated}
              capabilities={props.capabilities}
              autonomy={props.autonomy}
              fromRun={props.fromRun}
            />
          )}
        />
      </MemoryRouter>
    </ExecutionStoreProvider>
  );
}

function mount(
  props: { capabilities?: () => RunnerCapabilities[]; onCreated?: (id: string) => void; autonomy?: 'L3'; fromRun?: string } = {},
  fetchOpts: Parameters<typeof mockFetch>[0] = {},
) {
  vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch(fetchOpts));
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <Host {...props} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

// Only `FieldShell`-rendered labels carry a `for` attribute (the plain
// "Fleet" / "Exact runner" / model-mode radio labels wrap their `<input>`
// directly instead, per native `<label>` association) — filtering on that
// attribute avoids matching a radio's own label text by substring.
function select(labelText: string): HTMLSelectElement {
  const label = [...document.querySelectorAll('label[for]')].find((l) => l.textContent?.trim().startsWith(labelText));
  const id = label!.getAttribute('for')!;
  return document.getElementById(id) as HTMLSelectElement;
}

function field(labelText: string): HTMLInputElement {
  const label = [...document.querySelectorAll('label[for]')].find((l) => l.textContent?.trim().startsWith(labelText));
  const id = label!.getAttribute('for')!;
  return document.getElementById(id) as HTMLInputElement;
}

function setSelect(el: HTMLSelectElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
}

function setField(el: HTMLInputElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

function submitButton(): HTMLButtonElement {
  return [...document.querySelectorAll('button')].find((b) => b.textContent === 'Start run') as HTMLButtonElement;
}

function teardown() {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
}

const modelOptions = () => [...select('Model').options].map((o) => o.textContent);

afterEach(() => {
  teardown();
  document.body.innerHTML = '';
  vi.useRealTimers();
  vi.restoreAllMocks();
  lastCreateBody = undefined;
  lastItemPatch = undefined;
  requestedUrls = [];
});

describe('RunWithAgentModal', () => {
  it('a run carrying a level keeps to docket: the other agents are disabled with the reason, and the level is explained', async () => {
    const base = runnerCapabilitySnapshot();
    const both = runnerRow('runner-5', 'Both', runnerCapabilitySnapshot({
      harnesses: [...base.harnesses, { ...base.harnesses[0], harness_kind: 'docket' }],
    }));
    mount({ autonomy: 'L3' }, { runners: [both], fleets: [], project: { ...PROJECT, default_harness: 'codex' } });
    await flush();
    await flush();
    await flush();
    expect(chosenAgent()).toBe('docket');
    expect(agentButton('codex').disabled).toBe(true);
    expect(agentButton('codex').title).toBe('A level applies only to a docket run');
    expect(agentButton('docket').disabled).toBe(false);
    expect(document.querySelector('[data-testid="run-autonomy-note"]')!.textContent).toContain('you approve each step that needs it');
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ requested_harness_kind: 'docket', autonomy: 'L3' });
  });

  it('a fromRun prop sends from_run on the created request (decision 7, "Start over from this run\'s work")', async () => {
    mount({ fromRun: 'req-stopped-1' }, { runners: [RUNNER], fleets: [] });
    await flush();
    await flush();
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ from_run: 'req-stopped-1' });
    teardown();
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    await flush();
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).not.toHaveProperty('from_run');
  });

  it('a failed list of profiles is said as one with Try again, not offered as no profile', async () => {
    const profilesFailing = { down: true };
    mount({}, { runners: [RUNNER], fleets: [], profilesFailing });
    await flush();
    await flush();
    expect(document.body.textContent).toContain('Could not load the profiles');
    expect(submitButton().disabled).toBe(true);
    profilesFailing.down = false;
    ([...document.querySelectorAll('button')].find((b) => b.textContent === 'Try again') as HTMLButtonElement).click();
    await flush();
    await flush();
    expect(document.body.textContent).not.toContain('Could not load the profiles');
  });

  it('advanced_values_are_remembered_on_the_task', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    await flush();
    setField(field('Timeout'), '120');
    submitButton().click();
    await flush();
    await flush();
    // Only the key that differs from the defaults is stored; never the idempotency key.
    expect(lastItemPatch).toEqual({ run_settings: { timeout_seconds: 120 } });
    teardown();
    // Re-opening the task shows what it remembered.
    mount({}, { runners: [RUNNER], fleets: [], runSettings: { timeout_seconds: 120 } });
    await flush();
    await flush();
    expect(field('Timeout').value).toBe('120');
  });

  it('pre_fills_the_limits_from_the_profile_and_sends_a_token_limit_only_when_changed', async () => {
    const limits = { budgets: { tokens: 300000 }, timeout_seconds: 1800 };
    mount({}, { runners: [RUNNER], fleets: [], limits });
    await flush();
    await flush();
    expect(field('Token limit').value).toBe('300000');
    expect(field('Timeout').value).toBe('1800');
    expect(document.body.textContent).toContain('From the profile Reviewer');
    submitButton().click();
    await flush();
    await flush();
    // Untouched: the server takes the profile's own limit, and the time shown is sent.
    expect(lastCreateBody).toMatchObject({ budgets: {}, timeout_seconds: 1800 });
    teardown();
    mount({}, { runners: [RUNNER], fleets: [], limits });
    await flush();
    await flush();
    setField(field('Token limit'), '');
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ budgets: { tokens: null }, timeout_seconds: 1800 });
    // Nothing about the time was changed, so the task does not remember it.
    expect(lastItemPatch).toBeUndefined();
  });

  it('prints_the_servers_limit_sentence_under_the_field_and_asks_again_when_the_field_or_the_agent_changes', async () => {
    // The limit is sent after a pause; fake the clock so the test moves it instead of sleeping.
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const both = runnerRow('runner-4', 'Both', runnerCapabilitySnapshot({
      harnesses: [...runnerCapabilitySnapshot().harnesses, { ...runnerCapabilitySnapshot().harnesses[0], harness_kind: 'claude-code' }],
    }));
    const limits = { budgets: { tokens: 300000 } };
    mount({}, { runners: [both], fleets: [], limits });
    await flush();
    await flush();
    const sentence = () => document.querySelector('[data-testid="run-dialog-limit"]')?.textContent;
    const limitUrls = () => requestedUrls.filter((u) => u.includes('/agent-context') && u.includes('harness='));
    const textUrls = () => requestedUrls.filter((u) => u.includes('/agent-context') && !u.includes('harness='));
    // Untouched: no limit is sent, the profile's stands; the sentence is the server's, as given.
    expect(sentence()).toBe('Server says ?profile_id=profile-1&harness=codex');
    const textFetches = textUrls().length;
    // A typed limit goes up as `tokens`, after a pause, and the composed text is not asked for again.
    setField(field('Token limit'), '120000');
    await flush();
    expect(sentence()).toBe('Server says ?profile_id=profile-1&harness=codex');
    await vi.advanceTimersByTimeAsync(450);
    await flush();
    expect(sentence()).toBe('Server says ?profile_id=profile-1&harness=codex&tokens=120000');
    // The agent chosen goes up too.
    agentButton('claude-code').click();
    await flush();
    await flush();
    expect(sentence()).toBe('Server says ?profile_id=profile-1&harness=claude-code&tokens=120000');
    // A cleared field says "no limit".
    setField(field('Token limit'), '');
    await vi.advanceTimersByTimeAsync(450);
    await flush();
    expect(sentence()).toBe('Server says ?profile_id=profile-1&harness=claude-code&no_limit=true');
    expect(limitUrls().every((u) => u.startsWith('/api/items/item-1/agent-context?profile_id=profile-1&harness='))).toBe(true);
    expect(textUrls().length).toBe(textFetches);
  });

  it('prints_no_limit_sentence_while_the_token_field_holds_an_invalid_value', async () => {
    // The limit is sent after a pause; fake the clock so the test moves it instead of sleeping.
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    mount({}, { runners: [RUNNER], fleets: [], limits: { budgets: { tokens: 300000 } } });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-limit"]')).not.toBeNull();
    const before = requestedUrls.length;
    setField(field('Token limit'), '0');
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-limit"]')).toBeNull();
    await vi.advanceTimersByTimeAsync(450);
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-limit"]')).toBeNull();
    expect(document.body.textContent).toContain('Token limit must be a whole number above zero');
    // The server was never asked about a value it would refuse.
    expect(requestedUrls.slice(before).some((u) => u.includes('tokens=0'))).toBe(false);
    setField(field('Token limit'), '5000');
    await vi.advanceTimersByTimeAsync(450);
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-limit"]')?.textContent).toContain('tokens=5000');
  });

  it('asks_the_server_for_the_profile_and_says_who_chose_it', async () => {
    mount({}, {
      runners: [RUNNER], fleets: [], agentProfiles: [PROFILE, PROFILE_2],
      project: { ...PROJECT, default_profile_id: 'profile-1' },
      resolved: { profile_id: 'profile-2', profile_source: 'role:Content' },
    });
    await flush();
    await flush();
    const pill = profileRadio('Fixer');
    expect(pill.checked).toBe(true);
    expect(document.querySelector('[data-testid="profile-source"]')?.textContent).toBe('Chosen by the role Content');
    // A profile the person picks instead is theirs, not the server's.
    (profileRadio('Reviewer')).click();
    await flush();
    expect(document.querySelector('[data-testid="profile-source"]')).toBeNull();
  });

  it('only_a_picked_profile_is_remembered_on_the_task', async () => {
    const opts = { runners: [RUNNER], fleets: [], agentProfiles: [PROFILE], project: { ...PROJECT, default_profile_id: null } };
    mount({}, opts);
    await flush();
    await flush();
    setField(field('Timeout'), '120');
    submitButton().click();
    await flush();
    await flush();
    expect(lastItemPatch).toEqual({ run_settings: { timeout_seconds: 120 } });
    teardown();
    mount({}, opts);
    await flush();
    await flush();
    const pill = profileRadio('Reviewer');
    pill.click();
    submitButton().click();
    await flush();
    await flush();
    expect(lastItemPatch).toEqual({ run_settings: { agent_profile_id: 'profile-1' } });
  });

  it('a_configured_project_opens_ready_to_run', async () => {
    const claude = runnerRow('runner-3', 'Claude box', runnerCapabilitySnapshot({
      harnesses: [{
        harness_kind: 'claude-code', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z',
        requires_model: false,
      }],
    }));
    mount({}, { runners: [claude], fleets: [], agentContext: 'Fix login bug\n\nThe brief.' });
    await flush();
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    // Nothing to fill: Run is enabled as it opens.
    expect(submitButton().disabled).toBe(false);
    const summary = dialog.querySelector('[data-testid="run-summary"]')!;
    expect(summary.textContent).toContain('Reviewer');
    expect(summary.textContent).toContain("the agent's default");
    expect(summary.textContent).toContain('on a new branch of ~/src/app');
    for (const a of summary.querySelectorAll('a')) expect(a.getAttribute('href')).toBe('/projects/project-1/settings?tab=automation');
    expect(dialog.textContent).toContain('What the agent will read');
    expect(dialog.textContent).toContain('The brief.');
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).not.toHaveProperty('repository_snapshot');
  });

  it.each([
    [undefined, true],
    [{ acceptance: [] }, true],
    [{ acceptance: [{ id: 'a1', kind: 'command', title: 'tests pass', run: 'npm test', expect_exit: 0 }] }, false],
  ])('a card with no acceptance check says so, and Run stays enabled (brief %j)', async (brief, notice) => {
    const claude = runnerRow('runner-3', 'Claude box', runnerCapabilitySnapshot({
      harnesses: [{ harness_kind: 'claude-code', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z', requires_model: false }],
    }));
    mount({}, { runners: [claude], fleets: [], brief });
    await flush();
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent?.includes('This card has no acceptance check')).toBe(notice);
    expect(submitButton().disabled).toBe(false);
  });

  it('a project with no code is blocked by one sentence and a link, and has no Run', async () => {
    mount({}, { project: { ...PROJECT, code_origin: 'none', repository: null } });
    await flush();
    await flush();
    expect(submitButton()).toBeUndefined();
    const link = [...document.querySelectorAll('a')].find((a) => a.textContent === "Choose where this project's code is →")!;
    expect(link.getAttribute('href')).toBe('/projects/project-1/settings?tab=automation');
  });

  it('renders with the item name in the title, and every always-present section labeled', async () => {
    mount();
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Run with agent: Fix login bug');
    for (const section of ['What the agent will read', 'Ask before each action', 'Web tools', 'Advanced for this run']) {
      expect(dialog.textContent).toContain(section);
    }
  });

  it('"agent execution is off" renders instead of the form when zero runners have ever enrolled', async () => {
    mount({}, { runners: [] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Agent execution is off');
    expect(dialog.textContent).toContain('Turn it on');
    expect(dialog.querySelector('form')).toBeNull();
  });

  it('hides the target picker and auto-selects the one active runner when it is the only target — no free-text id anywhere', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).not.toContain('Machine or group');
    await flush();
    expect(submitButton().disabled).toBe(false);
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ selector_kind: 'exact_runner', selector_id: 'runner-1' });
  });

  it('a runner that does not answer is shown disabled with since when, and is never the fixed line', async () => {
    const silent = { ...runnerRow('runner-2', 'Build box', runnerCapabilitySnapshot()), online: false };
    mount({}, { runners: [RUNNER, silent], fleets: [] });
    await flush();
    await flush();
    const option = [...select('Machine or group').options].find((o) => o.value === 'exact_runner:runner-2')!;
    expect(option.disabled).toBe(true);
    expect(option.textContent).toMatch(/^Build box \(not answering since .+\)$/);
    expect(document.querySelector('[data-testid="run-target-fixed"]')).toBeNull();
    // The one that answers is still chosen for the person.
    expect(select('Machine or group').value).toBe('exact_runner:runner-1');
  });

  it('when no computer answers, the field says which and since when, and the footer links to turning execution on', async () => {
    const silent = { ...runnerRow('runner-2', 'This computer (Pet1)', runnerCapabilitySnapshot()), online: false };
    mount({}, { runners: [silent], fleets: [] });
    await flush();
    await flush();
    const line = document.querySelector('[data-testid="run-target-silent"]')!;
    expect(line.textContent).toContain('Agents offline');
    expect(line.textContent).toMatch(/This computer \(Pet1\) has not answered since .+, so no run can start\./);
    expect(line.querySelector('a')!.getAttribute('href')).toBe('/agents');
    // No select with nothing to choose, and no "Select where this runs." repeating it.
    expect([...document.querySelectorAll('label[for]')].some((l) => l.textContent?.includes('Machine or group'))).toBe(false);
    expect(document.body.textContent).not.toContain('Select where this runs.');
    const footer = document.querySelector('[data-testid="run-footer"]')!;
    expect(footer.querySelector('a')!.textContent).toBe('Turn on agent execution →');
    expect(footer.querySelector('[data-testid="run-start"]')).toBeNull();
  });

  it('one answering runner and no group is one fixed line, "Runs on this computer" for this computer', async () => {
    mount({}, { runners: [{ ...RUNNER, name: 'This computer (dev)' }], fleets: [] });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="run-target-fixed"]')?.textContent).toBe('Runs on this computer');
    expect(document.querySelector('label[for]')?.textContent).not.toContain('Machine or group');
  });

  it('an agent offers only its own profiles, and keeps the chosen one only when that agent runs it', async () => {
    const both = runnerRow('runner-9', 'Two agents', runnerCapabilitySnapshot({
      harnesses: ['codex', 'claude-code'].map((harness_kind) => ({
        harness_kind, installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z', requires_model: false,
      })),
    }));
    const claudeProfile = { ...PROFILE_2, tool_policy: { tools: { 'claude-code': ['Read'] } } };
    const codexProfile = { ...PROFILE_2, agent_profile_id: 'profile-3', name: 'Coder', tool_policy: { agent: 'codex' } };
    mount({}, {
      runners: [both], fleets: [], agentProfiles: [PROFILE, claudeProfile, codexProfile],
      project: { ...PROJECT, default_profile_id: 'profile-1' }, runSettings: { harness: 'codex' },
    });
    await flush();
    await flush();
    const names = () => [...document.querySelectorAll('[data-testid="who-does-it"] label')].map((l) => l.textContent);
    // Codex is chosen: its profiles and the one that runs on any agent, not Claude Code's.
    expect(chosenAgent()).toBe('codex');
    expect(names()).toEqual(['Reviewer', 'Coder']);
    expect(profileRadio('Reviewer').checked).toBe(true);
    // A profile any agent runs stays chosen when the agent changes.
    agentButton('claude-code').click();
    await flush();
    expect(names()).toEqual(['Reviewer', 'Fixer']);
    expect(profileRadio('Reviewer').checked).toBe(true);
    // One the new agent does not run is let go, and the person picks again.
    agentButton('codex').click();
    await flush();
    profileRadio('Coder').click();
    await flush();
    agentButton('claude-code').click();
    await flush();
    expect(names()).toEqual(['Reviewer', 'Fixer']);
    expect(document.querySelector('[data-testid="who-does-it"] input:checked')).toBeNull();
    expect(document.body.textContent).toContain('Choose a profile.');
    expect(submitButton().disabled).toBe(true);
  });

  it('asks for the agent first, and under it lists only that agent\'s profiles, with no Built in list', async () => {
    const mine = { ...PROFILE_2, name: 'Fixer', delivers: 'a change on a branch', tool_policy: { agent: 'codex' } };
    const builtIn = { ...PROFILE, name: 'Reviewer', builtin: true, kind: 'reviewer', summary: 'Reads the change and says what is wrong', tool_policy: { agent: 'claude-code' } };
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [builtIn, mine], project: { ...PROJECT, default_profile_id: 'profile-2' } });
    await flush();
    await flush();
    const picker = document.querySelector('[data-testid="who-does-it"]')!;
    expect(picker).not.toBeNull();
    // A Run needs a profile: there is no "Nobody yet" here.
    expect(picker.textContent).not.toContain('Nobody yet');
    expect(picker.textContent).not.toContain('Built in');
    expect(picker.textContent).not.toContain('Your profiles');
    // The computer reports Codex; Claude Code, which a profile names, stays in the list, disabled, with the reason.
    expect(agentChoices()).toEqual([['claude-code', 'Claude Code. Not reported by the computer this runs on'], ['codex', 'Codex']]);
    expect(agentButton('claude-code').disabled).toBe(true);
    expect(agentButton('claude-code').title).toBe('Not reported by the computer this runs on');
    expect(chosenAgent()).toBe('codex');
    // Only Codex's profile is offered, with its note; Claude Code's is not beside it.
    const codex = picker.querySelector('[data-testid="who-alone-codex"]')!;
    expect(codex.textContent).toContain('Fixer');
    expect(codex.textContent).toContain('a change on a branch');
    expect(picker.textContent).not.toContain('Reads the change and says what is wrong');
    expect(picker.textContent).not.toContain('A single agent, no team');
    expect(picker.textContent).toContain('Each task can use a different agent; this choice is for this task.');
  });

  it('says what an agent can do from its own report, and greys what it cannot with its reason', async () => {
    const mine = { ...PROFILE_2, name: 'Fixer', tool_policy: { agent: 'codex' } };
    const runner = runnerRow('runner-1', 'This computer', runnerCapabilitySnapshot({
      harnesses: [{
        harness_kind: 'codex', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z',
        features: {
          cancel: { support: 'supported', reason: null },
          decisions: { support: 'supported', reason: null },
          proposals: { support: 'unsupported', reason: 'Codex has no plan file here' },
        },
      }],
    }));
    mount({}, { runners: [runner], fleets: [], agentProfiles: [PROFILE, mine] });
    await flush();
    await flush();
    // Said once, under the agent, not in each profile's row.
    const codex = document.querySelector('[data-testid="who-can"]')!;
    expect(codex.textContent).toContain('Codex can stop a run and answer questions.');
    const icon = (id: string) => codex.querySelector<HTMLElement>(`[data-capability="${id}"]`)!;
    // What the agent can do is a plain icon; its name says so.
    expect(icon('cancel').dataset.state).toBe('can');
    expect(icon('cancel').getAttribute('aria-label')).toBe('Can stop a run');
    expect(icon('cancel').querySelector('svg path[d="M2.5 13.5L13.5 2.5"]')).toBeNull();
    // What it cannot is struck through, and the reason is the tooltip and the name.
    expect(icon('proposals').dataset.state).toBe('cannot');
    expect(icon('proposals').getAttribute('aria-label')).toBe('Cannot propose items · Codex has no plan file here');
    expect(icon('proposals').title).toBe('Cannot propose items · Codex has no plan file here');
    expect(icon('proposals').querySelector('svg path[d="M2.5 13.5L13.5 2.5"]')).not.toBeNull();
    // A feature the agent left out is "not reported", never filled from the computer's own report.
    expect(icon('usage').dataset.state).toBe('not_reported');
    expect(icon('usage').title).toBe('Not reported by Codex');
    expect(icon('usage').querySelector('svg path[d="M2.5 13.5L13.5 2.5"]')).not.toBeNull();
  });

  it('shows a team with its members as chips, and a reported team no profile names as a disabled row', async () => {
    const teamProfile = { ...PROFILE, name: 'Content team', tool_policy: { agent: 'docket', docket: { team: 'saas-content' } } };
    const teams = [
      { name: 'saas-content', harness: 'docket', reported: true, profiles: [], report: { members: [{ role: 'Lead', model: null }, { role: 'Writer', model: null }] } },
      { name: 'saas-code', harness: 'docket', reported: true, profiles: [], report: { members: [{ role: 'Builder', model: null }] } },
    ];
    vi.spyOn(globalThis, 'fetch').mockImplementation(((input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input).endsWith('/teams')) return Promise.resolve(new Response(JSON.stringify({ teams }), { status: 200 }));
      const both = runnerRow('runner-td', 'Both', runnerCapabilitySnapshot({
        harnesses: [...runnerCapabilitySnapshot().harnesses, { ...runnerCapabilitySnapshot().harnesses[0], harness_kind: 'docket' }],
      }));
      return (mockFetch({ runners: [both], fleets: [], agentProfiles: [teamProfile, PROFILE_2] }) as typeof fetch)(input, init);
    }) as typeof fetch);
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(() => <Host />, container);
    disposers.push(() => { dispose(); container.remove(); });
    await flush();
    await flush();
    const content = document.querySelector('[data-testid="who-team-saas-content"]')!;
    expect(content.textContent).toContain('Team saas-content');
    expect([...content.querySelectorAll('[aria-label="Made of"] li')].map((li) => li.textContent)).toEqual(['Lead', 'Writer']);
    expect(content.textContent).toContain('Content team');
    // A reported team no profile names is not a group of its own: it is one disabled row.
    expect(document.querySelector('[data-testid="who-team-saas-code"]')).toBeNull();
    const bare = document.querySelector('[data-testid="who-teams-without-profile"]')!;
    expect(bare.textContent).toContain('1 team has no profile yet');
    expect(bare.textContent).toContain('saas-code');
    expect(bare.querySelector('input[type="radio"]')!.getAttribute('aria-label')).toContain('Make a profile for it on the Agents page');
    expect(bare.querySelector('input[type="radio"]')!.hasAttribute('disabled')).toBe(true);
    // The words of the engine never reach the screen.
    expect(document.querySelector('[data-testid="who-does-it"]')!.textContent).not.toMatch(/harness|pod|blueprint|archetype/i);
  });

  it('a profile that runs no commands says nothing is installed', async () => {
    const reader = { ...PROFILE, runs_commands: false };
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [reader] });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-install"]')?.textContent).toBe('No install: this profile runs no commands');
  });

  it('a profile whose team works in a folder says so in place of the branch, and installs nothing', async () => {
    const folderProfile = { ...PROFILE, tool_policy: { docket: { team: 'saas-content' } } };
    const teams = [{ name: 'saas-content', profile: 'lights-out', caps_at: null, fix: null, works_in: 'folder' }];
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [folderProfile], autonomy: { teams } });
    await flush();
    await flush();
    const summary = document.querySelector('[data-testid="run-summary"]')!;
    expect(summary.textContent).toContain("in the team's own folder on this computer, not in this repository");
    expect(summary.textContent).not.toContain('on a new branch of');
    expect(document.querySelector('[data-testid="run-dialog-install"]')).toBeNull();
  });

  it('pressing a profile with no team puts the branch and the install line back', async () => {
    const folderProfile = { ...PROFILE, tool_policy: { docket: { team: 'saas-content' } } };
    const teams = [{ name: 'saas-content', profile: 'lights-out', caps_at: null, fix: null, works_in: 'folder' }];
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [folderProfile, PROFILE_2], autonomy: { teams } });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="run-summary"]')!.textContent).toContain("in the team's own folder");
    (profileRadio('Fixer')).click();
    await flush();
    expect(document.querySelector('[data-testid="run-summary"]')!.textContent).toContain('on a new branch of ~/src/app');
    expect(document.querySelector('[data-testid="run-dialog-install"]')).not.toBeNull();
  });

  it('shows a combined machine/group picker, by name, when more than one target exists', async () => {
    mount({}, { runners: [RUNNER], fleets: [FLEET] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    const picker = select('Machine or group');
    const labels = [...picker.options].map((o) => o.textContent);
    expect(labels).toContain(FLEET.name);
    expect(labels).toContain(RUNNER.name);
    // Never a raw id in the option text.
    expect(labels.some((l) => l?.includes(RUNNER.runner_id))).toBe(false);
  });

  it('manually picking a specific runner from the combined picker actually selects it (not just an auto-selected single target)', async () => {
    // Regression coverage: an earlier version of this picker's `<option>`
    // values used a `runner:` prefix that its own `onInput` parser didn't
    // recognise (it checked for `exact_runner`), so a real pick silently
    // cleared the selection — the native `<select>` still *displayed* the
    // picked option (browser-owned DOM state), while `selectorId` reset to
    // `''` and the submit button stayed disabled forever. Only an E2E run
    // against the real picker caught it; this test pins the fix so it can't
    // regress silently again.
    mount({}, { runners: [RUNNER], fleets: [FLEET] });
    await flush();
    setSelect(select('Machine or group'), `exact_runner:${RUNNER.runner_id}`);
    await flush();
    expect(submitButton().disabled).toBe(false);
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ selector_kind: 'exact_runner', selector_id: RUNNER.runner_id });
  });

  it('submit is disabled with every missing-field reason listed when the form is empty', async () => {
    mount({}, { runners: [RUNNER], fleets: [FLEET], agentProfiles: [PROFILE, PROFILE_2], project: { ...PROJECT, default_profile_id: null } });
    await flush();
    expect(submitButton().disabled).toBe(true);
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Select where this runs.');
    expect(dialog.textContent).toContain('Choose a profile.');
  });

  it('offers "Create default profile" inline when no agent profile exists yet, and selects the new one', async () => {
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('No agent profile exists yet.');
    const createBtn = [...dialog.querySelectorAll('button')].find((b) => b.textContent === 'Create default profile')!;
    createBtn.click();
    await flush();
    await flush();
    expect(dialog.textContent).not.toContain('No agent profile exists yet.');
    const pill = profileRadio('Default');
    expect(pill.checked).toBe(true);
  });

  it('the repository is not asked for; only a branch override sends a snapshot, built from the project', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.querySelector('input[placeholder="git@github.com:org/repo.git"]')).toBeNull();
    setField(field('Branch'), 'dev');
    await flush();
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({
      repository_snapshot: { kind: 'git', remote: '/home/alice/src/app', base_revision: 'dev', subdirectory: null },
    });
  });

  it('with a project model default for the default agent, "Project default — …" is shown and applied', async () => {
    mount({}, { runners: [RUNNER], fleets: [], project: { id: 'project-1', name: 'P', default_harness: 'codex', default_model: 'opaque/model-alpha' } });
    await flush();
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Project default — opaque/model-alpha');
    expect(select('Model').value).toBe('');
    submitButton().click();
    await flush();
    await flush();
    // The request names none: the server fills the project's default.
    expect(lastCreateBody).toMatchObject({ requested_model: null });
  });

  it('a project model default is not offered for an agent other than the project\'s default agent', async () => {
    mount({}, { runners: [RUNNER], fleets: [], project: { id: 'project-1', name: 'P', default_harness: 'claude-code', default_model: 'claude-opus-5-5' } });
    await flush();
    await flush();
    expect(document.querySelector('[role="dialog"]')!.textContent).not.toContain('Project default —');
  });

  it('with no project model default, the agent\'s default is selected and no "Project default" line is shown', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).not.toContain('Project default —');
    expect(select('Model').value).toBe('');
    expect(modelOptions()[0]).toBe("Agent's default");
  });

  it('an agent that needs a model, with none named, says why in place of Run, with a link to set one', async () => {
    const needs = runnerRow('runner-5', 'Box', runnerCapabilitySnapshot({ harnesses: [{
      harness_kind: 'codex', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z', requires_model: true,
      requires_model_reason: 'Codex has no model of its own on this computer.',
    }] }));
    mount({}, { runners: [needs], fleets: [], agentProfiles: [PROFILE_2] });
    await flush();
    await flush();
    expect(submitButton()).toBeUndefined();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(select('Model').value).toBe('');
    expect(modelOptions()[0]).toBe('Choose a model');
    expect(dialog.querySelector('[data-prerequisite="missing"]')?.textContent).toContain('Codex has no model of its own on this computer.');
    expect(dialog.textContent).toContain('Unsupported');
    const fixLink = [...dialog.querySelectorAll('a')].find((a) => a.textContent === 'Set a default model for this project');
    expect(fixLink!.getAttribute('href')).toBe('/projects/project-1/settings?tab=automation');
  });

  it('the model list is what the agent reports, and picking one enables Run', async () => {
    const claude = runnerRow('runner-3', 'Claude box', runnerCapabilitySnapshot({
      harnesses: [{
        harness_kind: 'claude-code', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z', requires_model: true,
        models: { source: 'listed', ids: ['claude-sonnet-5-5', 'claude-opus-5-5'], reason: null },
        docs: { models: 'https://example.test/models', sign_in: 'https://example.test/sign-in' },
      }],
    }));
    mount({}, { runners: [claude], fleets: [] });
    await flush();
    await flush();
    expect(modelOptions()).toEqual(['Choose a model', 'claude-sonnet-5-5', 'claude-opus-5-5', 'Another model…']);
    expect(document.querySelector('[data-testid="model-line"]')?.textContent).toContain('From Claude Code on this computer.');
    expect(submitButton()).toBeUndefined();
    setSelect(select('Model'), 'claude-sonnet-5-5');
    await flush();
    expect(submitButton().disabled).toBe(false);
    expect(document.body.textContent).toContain('Runs on claude-sonnet-5-5');
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ requested_model: 'claude-sonnet-5-5' });
  });

  it('an agent that needs no model opens on its default and sends none', async () => {
    const claude = runnerRow('runner-3', 'Claude box', runnerCapabilitySnapshot({
      harnesses: [{
        harness_kind: 'claude-code', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z',
        requires_model: false, default_model: 'claude-sonnet-5-5',
      }],
    }));
    mount({}, { runners: [claude], fleets: [] });
    await flush();
    await flush();
    expect(modelOptions()[0]).toBe("Agent's default (claude-sonnet-5-5)");
    expect(document.body.textContent).toContain("Uses the agent's own model");
    expect(submitButton().disabled).toBe(false);
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ requested_model: null });
  });

  it('the_project_default_harness_that_needs_no_model_is_not_left_blocked', async () => {
    // The form starts on codex (required) until the project's claude-code (optional) lands.
    const harness = (harness_kind: string, requires_model: boolean) => ({
      harness_kind, installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z',
      requires_model,
    });
    const row = runnerRow('runner-4', 'Box', runnerCapabilitySnapshot({ harnesses: [harness('codex', true), harness('claude-code', false)] }));
    mount({}, { runners: [row], fleets: [], project: { ...PROJECT, default_harness: 'claude-code' }, itemDelayMs: 10 });
    await new Promise((r) => setTimeout(r, 30));
    await flush();
    expect(modelOptions()[0]).toBe("Agent's default");
    expect(document.querySelector('[data-testid="run-summary"]')?.textContent).toContain("the agent's default");
    expect(submitButton().disabled).toBe(false);
  });

  it('a_harness_that_needs_a_model_says_so_where_run_was', async () => {
    const codex = runnerRow('runner-5', 'Box', runnerCapabilitySnapshot({ harnesses: [{
      harness_kind: 'codex', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z',
      requires_model: true, requires_model_reason: 'Codex needs a model named here.',
    }] }));
    mount({}, { runners: [codex], fleets: [], project: { ...PROJECT, default_harness: 'codex' } });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="run-summary"]')?.textContent).toContain('no model chosen');
    expect(submitButton()).toBeUndefined();
    expect(document.querySelector('[data-prerequisite="missing"]')?.textContent).toContain('Codex needs a model named here.');
    // The field is in view: the advanced section opens while the model is missing.
    expect((document.querySelectorAll('details')[1] as HTMLDetailsElement).open).toBe(true);
  });

  it('a typed model name goes out verbatim, with no provider anywhere', async () => {
    const row = runnerRow('runner-2', 'Box', runnerCapabilitySnapshot());
    mount({}, { runners: [row], fleets: [] });
    await flush();
    expect(modelOptions()).toEqual(["Agent's default", 'Another model…']);
    setSelect(select('Model'), '__other__');
    await flush();
    setField(field('Model name'), 'opaque/Typed Name:1');
    await flush();
    expect(document.body.textContent).not.toMatch(/Provider/);
    expect(document.body.textContent).not.toContain('Through');
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ requested_model: 'opaque/Typed Name:1' });
  });

  it('submitting posts the exact CreateExecutionInput shape, closes, and calls onCreated', async () => {
    const onCreated = vi.fn();
    mount({ onCreated }, { runners: [RUNNER], fleets: [] });
    await flush();
    await flush();
    expect(submitButton().disabled).toBe(false);
    submitButton().click();
    await flush();
    await flush();

    expect(lastCreateBody).toMatchObject({
      item_id: 'item-1',
      selector_kind: 'exact_runner',
      selector_id: 'runner-1',
      agent_profile_id: 'profile-1',
      requested_harness_kind: 'codex',
      requested_model: null,
      permission_policy: { tools: [], network: false },
    });
    expect(lastCreateBody).not.toHaveProperty('repository_snapshot');
    expect(typeof (lastCreateBody as Record<string, unknown>).idempotency_key).toBe('string');
    expect(onCreated).toHaveBeenCalledWith('req-9');
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it('a_blank_tools_field_sends_the_profiles_tools_for_the_agent', async () => {
    const withTools = { ...PROFILE, tool_policy: { tools: { codex: ['*'], 'claude-code': ['Read', 'Edit'] } } };
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [withTools] });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="no-tools"]')).toBeNull();
    expect(field('Allowed tools').placeholder).toBe('all tools');
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ permission_policy: { tools: ['*'] } });
    teardown();
    // A typed list wins over the profile's.
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [withTools] });
    await flush();
    await flush();
    setField(field('Allowed tools'), 'bash');
    await flush();
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ permission_policy: { tools: ['bash'] } });
  });

  it('a_profile_that_gives_the_agent_no_tools_says_so_before_the_run', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="no-tools"]')?.textContent).toContain('gives Codex no tools');
  });

  it('a_deferred_capability_is_shown_disabled_with_its_reason', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    const rows = [...document.querySelectorAll('[data-prerequisite="deferred"]')];
    const reasons = [
      'This runner has no verifier set up',
      'This runner does not push branches',
      'Needs a pushed branch',
    ];
    expect(rows.length).toBe(3);
    for (const row of rows) {
      const control = row.querySelector('input') as HTMLInputElement;
      expect(control.disabled).toBe(true);
      expect(reasons.some((r) => row.textContent?.includes(r))).toBe(true);
    }
  });

  it('the verify checkbox is live only with the flag reported, and unticking it posts verify: false', async () => {
    const verifying = runnerRow('runner-3', 'Verifier box', runnerCapabilitySnapshot({ verify_configured: true, push_configured: false }));
    mount({}, { runners: [verifying], fleets: [] });
    await flush();
    const row = (label: string) => [...document.querySelectorAll('[data-prerequisite]')].find((r) => r.textContent?.includes(label))!;
    const box = (label: string) => row(label).querySelector('input') as HTMLInputElement;
    expect(row('Verify the result').getAttribute('data-prerequisite')).toBe('ok');
    expect(box('Verify the result').disabled).toBe(false);
    expect(row('Push the branch').getAttribute('data-prerequisite')).toBe('deferred');
    expect(box('Push the branch').disabled).toBe(true);
    box('Verify the result').click();
    await flush();
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ verify: false });
    expect(lastCreateBody).not.toHaveProperty('push_branch');
  });

  it('every missing row links to the page that fixes it', async () => {
    const noHarness = runnerRow('runner-2', 'Bare', runnerCapabilitySnapshot({ harnesses: [] }));
    const cases: Array<[Parameters<typeof mockFetch>[0], string, string]> = [
      [{ runners: [RUNNER], fleets: [], agentProfiles: [] }, 'An agent profile exists', '/agents'],
      [{ runners: [noHarness], fleets: [] }, 'The agent is installed and signed in', '/agents'],
    ];
    for (const [fetchOpts, label, href] of cases) {
      mount({}, fetchOpts);
      await flush();
      const row = [...document.querySelectorAll('[data-prerequisite="missing"]')].find((r) => r.textContent?.includes(label));
      expect(row, label).toBeTruthy();
      expect(row!.querySelector('a')!.getAttribute('href')).toBe(href);
      teardown();
    }
  });

  it('Run is disabled with one missing row and enabled with only deferred ones', async () => {
    const project = { id: 'project-1', name: 'P', default_model: 'opaque/model-alpha' };
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [], project });
    await flush();
    await flush();
    expect(document.querySelectorAll('[data-prerequisite="missing"]').length).toBe(1);
    expect(submitButton().disabled).toBe(true);
    [...document.querySelectorAll('button')].find((b) => b.textContent === 'Create default profile')!.click();
    await flush();
    await flush();
    expect(document.querySelectorAll('[data-prerequisite="missing"]').length).toBe(0);
    expect(document.querySelectorAll('[data-prerequisite="deferred"]').length).toBe(3);
    expect(submitButton().disabled).toBe(false);
  });

  it('the_approvals_default_follows_the_sandbox_and_a_remembered_value_wins', async () => {
    const sandboxed = (sandbox: string) => runnerRow('runner-7', 'Box', runnerCapabilitySnapshot({
      harnesses: [{ ...runnerCapabilitySnapshot().harnesses[0], decisions: { support: 'supported', reason: null }, sandbox, sandbox_reason: sandbox === 'none' ? 'bwrap not on PATH' : undefined }],
    }));
    const ask = () => (document.querySelector('input[name="approvals"]') as HTMLInputElement).checked;
    const line = () => document.querySelector('[data-testid="sandbox-line"]')?.textContent ?? '';
    mount({}, { runners: [sandboxed('none')], fleets: [] });
    await flush(); await flush();
    expect(ask()).toBe(true);
    expect(line()).toContain('Not sandboxed on this computer, so the agent asks before each action');
    teardown();
    mount({}, { runners: [sandboxed('bwrap')], fleets: [] });
    await flush(); await flush();
    expect(ask()).toBe(false);
    expect(line()).toContain('Sandboxed: the agent only reaches its workspace');
    teardown();
    mount({}, { runners: [sandboxed('none')], fleets: [], runSettings: { approvals: 'auto' } });
    await flush(); await flush();
    expect(ask()).toBe(false);
  });

  it('the_web_tools_box_starts_at_the_profiles_network_and_a_remembered_or_touched_value_wins', async () => {
    const researcher = { ...PROFILE, agent_profile_id: 'profile-net', name: 'Researcher', tool_policy: { network: true }, default_network: { codex: true } };
    const web = () => ([...document.querySelectorAll('label')].find((l) => l.textContent?.trim() === 'Web tools')!.querySelector('input') as HTMLInputElement);
    const pick = (name: string) => (profileRadio(name)).click();
    const opts = { runners: [RUNNER], fleets: [], agentProfiles: [PROFILE, researcher], project: { ...PROJECT, default_profile_id: null } };
    // A profile without the key: off, and the run says so.
    mount({}, opts);
    await flush(); await flush();
    pick('Reviewer');
    await flush();
    expect(web().checked).toBe(false);
    // Changing the profile refills the box while the person has not touched it.
    pick('Researcher');
    await flush();
    expect(web().checked).toBe(true);
    submitButton().click();
    await flush(); await flush();
    expect(lastCreateBody).toMatchObject({ permission_policy: { network: true } });
    // The profile's own network is not a choice the task has to remember.
    expect(JSON.stringify(lastItemPatch)).not.toContain('network');
    teardown();
    // A touched box stays as the person set it when the profile changes.
    mount({}, opts);
    await flush(); await flush();
    pick('Researcher');
    await flush();
    web().click();
    await flush();
    pick('Reviewer');
    await flush();
    expect(web().checked).toBe(false);
    pick('Researcher');
    await flush();
    expect(web().checked).toBe(false);
    submitButton().click();
    await flush(); await flush();
    expect(lastCreateBody).toMatchObject({ permission_policy: { network: false } });
    expect(lastItemPatch).toMatchObject({ run_settings: { network: false } });
    teardown();
    // What the task remembered wins over the profile.
    mount({}, { ...opts, runSettings: { agent_profile_id: 'profile-net', network: false } });
    await flush(); await flush();
    expect(web().checked).toBe(false);
  });

  it('the_web_tools_box_follows_the_servers_default_for_the_chosen_agent_and_opencode_locks_it_on', async () => {
    const web = () => ([...document.querySelectorAll('label')].find((l) => l.textContent?.trim() === 'Web tools')!.querySelector('input') as HTMLInputElement);
    // Tools that name the web are a default of the server's, not a rule the dialog repeats.
    const researcher = { ...PROFILE, agent_profile_id: 'profile-web', name: 'Researcher', tool_policy: { tools: { codex: ['WebFetch'] } }, default_network: { codex: true, opencode: true } };
    const codex = runnerRow('runner-9', 'Box', runnerCapabilitySnapshot());
    mount({}, { runners: [codex], fleets: [], agentProfiles: [PROFILE, researcher], project: { ...PROJECT, default_profile_id: null, default_harness: 'codex' } });
    await flush(); await flush();
    profileRadio('Researcher').click();
    await flush();
    expect(web().checked).toBe(true);
    expect(web().disabled).toBe(false);
    teardown();
    // opencode reaches the network on every run: the box is on, shut, and says why.
    const opencode = runnerRow('runner-10', 'Box', runnerCapabilitySnapshot({ harnesses: [{
      harness_kind: 'opencode', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z',
    }] }));
    mount({}, { runners: [opencode], fleets: [], agentProfiles: [PROFILE, researcher], project: { ...PROJECT, default_profile_id: null, default_harness: 'opencode' }, runSettings: { network: false } });
    await flush(); await flush();
    profileRadio('Reviewer').click();
    await flush();
    expect(web().checked).toBe(true);
    expect(web().disabled).toBe(true);
    expect(document.querySelector('[data-testid="web-tools-locked"]')!.textContent).toContain('opencode reaches the network on every run');
  });

  it('an agent with no sandbox that cannot ask says it does not run here, and Run stays off', async () => {
    const silent = runnerRow('runner-8', 'Box', runnerCapabilitySnapshot({
      harnesses: [{
        ...runnerCapabilitySnapshot().harnesses[0],
        decisions: { support: 'unsupported', reason: null },
        sandbox: 'none',
        sandbox_reason: 'bwrap not on PATH',
        probe_error: 'not sandboxed on this machine (bwrap not on PATH) and this agent cannot ask before it acts',
      }],
    }));
    mount({}, { runners: [silent], fleets: [] });
    await flush(); await flush();
    expect(document.querySelector('[data-testid="sandbox-line"]')?.textContent).toContain("Doesn't run on this computer: it isn't sandboxed and can't ask before it acts");
    expect(document.querySelectorAll('[data-prerequisite="missing"]').length).toBe(1);
    expect(submitButton()?.disabled ?? true).toBe(true);
  });

  it('the_request_sends_no_profile_snapshot_and_the_preview_names_the_profile', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush(); await flush();
    submitButton().click();
    await flush(); await flush();
    expect(lastCreateBody).not.toHaveProperty('agent_profile_snapshot');
    expect(requestedUrls.some((u) => u.includes('/agent-context?profile_id=profile-1'))).toBe(true);
  });

  it('"Ask me" stays gated by decisionsAttested()', async () => {
    const askRadio = () => document.querySelector('input[name="approvals"]') as HTMLInputElement;
    const harness = (decisions: string) => ({
      ...runnerCapabilitySnapshot().harnesses[0],
      decisions: { support: decisions, reason: null },
    });
    const off = runnerRow('runner-3', 'No ask', runnerCapabilitySnapshot({ harnesses: [harness('unsupported')] }));
    const on = runnerRow('runner-5', 'Can ask', runnerCapabilitySnapshot({ harnesses: [harness('supported')] }));
    mount({}, { runners: [off], fleets: [] });
    await flush();
    expect(askRadio().disabled).toBe(true);
    teardown();
    mount({}, { runners: [on], fleets: [] });
    await flush();
    expect(askRadio().disabled).toBe(false);
  });

  it('allowed tools: a checklist for claude-code, free text with "exposes no list" for codex', async () => {
    const both = runnerRow('runner-4', 'Both', runnerCapabilitySnapshot({
      harnesses: [...runnerCapabilitySnapshot().harnesses, { ...runnerCapabilitySnapshot().harnesses[0], harness_kind: 'claude-code' }],
    }));
    mount({}, { runners: [both], fleets: [] });
    await flush();
    expect(document.querySelector('[aria-label="Suggested tools"]')).toBeNull();
    expect(document.body.textContent).toContain('Codex exposes no list');
    agentButton('claude-code').click();
    await flush();
    expect(document.querySelectorAll('[aria-label="Suggested tools"] input').length).toBe(8);
    expect(field('Allowed tools')).toBeTruthy();
  });

  it('with only claude-code reported, the agent is claude-code — what the choice shows is what is sent', async () => {
    const claudeOnly = runnerRow('runner-6', 'Claude only', runnerCapabilitySnapshot({
      harnesses: [{ ...runnerCapabilitySnapshot().harnesses[0], harness_kind: 'claude-code' }],
    }));
    mount({}, { runners: [claudeOnly], fleets: [] });
    await flush();
    expect(chosenAgent()).toBe('claude-code');
    expect(agentChoices()).toEqual([['claude-code', 'Claude Code']]);
    expect(document.body.textContent).not.toContain('Codex exposes no list');
  });

  it('a docket-only runner offers docket under its label, so a docket runner can be run from the dialog', async () => {
    const docketOnly = runnerRow('runner-7', 'Docket only', runnerCapabilitySnapshot({
      harnesses: [{ ...runnerCapabilitySnapshot().harnesses[0], harness_kind: 'docket' }],
    }));
    mount({}, { runners: [docketOnly], fleets: [] });
    await flush();
    expect(agentChoices()).toEqual([['docket', 'Docket']]);
    expect(chosenAgent()).toBe('docket');
    expect(document.body.textContent).toContain('Docket exposes no list');
  });

  it('a reported harness outside HARNESS_KINDS is offered under its wire name', async () => {
    const unknownOnly = runnerRow('runner-8', 'Unknown only', runnerCapabilitySnapshot({
      harnesses: [{ ...runnerCapabilitySnapshot().harnesses[0], harness_kind: 'prill' }],
    }));
    mount({}, { runners: [unknownOnly], fleets: [] });
    await flush();
    expect(agentChoices()).toEqual([['prill', 'prill']]);
    expect(chosenAgent()).toBe('prill');
  });

  it('every visible field has an accessible label (native <label for>) — the keyboard/a11y path required here', async () => {
    mount({}, { runners: [RUNNER], fleets: [FLEET] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    const inputs = [...dialog.querySelectorAll('input[id], select[id]')];
    expect(inputs.length).toBeGreaterThan(0);
    for (const input of inputs) {
      const id = input.getAttribute('id')!;
      const label = dialog.querySelector(`label[for="${id}"]`);
      expect(label, `no label for #${id}`).toBeTruthy();
    }
  });
});

describe('the Run with agent dialog picks a team in one glance', () => {
  const DOCKET_RUNNER = runnerRow('runner-d', 'This computer', runnerCapabilitySnapshot({
    verify_configured: true,
    harnesses: [{
      harness_kind: 'docket', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z', requires_model: false,
      features: {
        cancel: { support: 'supported', reason: null },
        resume: { support: 'supported', reason: 'by task id' },
        decisions: { support: 'supported', reason: null },
        proposals: { support: 'unsupported', reason: 'No record set named tack_proposals' },
        usage: { support: 'supported', reason: null },
      },
    }],
  }));
  const teamProfile = (id: string, name: string, team: string) => ({ ...PROFILE, agent_profile_id: id, name, tool_policy: { agent: 'docket', docket: { team } } });
  const team = (name: string, extra: Record<string, unknown> = {}) => ({ name, harness: 'docket', reported: true, profiles: [], report: { members: [{ role: 'Lead' }], ...extra } });

  /** The dialog with the given teams reported by `GET /api/teams`. */
  async function mountWithTeams(teams: unknown[], fetchOpts: Parameters<typeof mockFetch>[0]) {
    vi.spyOn(globalThis, 'fetch').mockImplementation(((input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input).endsWith('/teams')) return Promise.resolve(new Response(JSON.stringify({ teams }), { status: 200 }));
      return (mockFetch(fetchOpts) as typeof fetch)(input, init);
    }) as typeof fetch);
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(() => <Host />, container);
    disposers.push(() => { dispose(); container.remove(); });
    await flush();
    await flush();
  }

  it('groups the profiles by team, each with its members and where it works, and says what the agent can do once', async () => {
    const teams = [
      team('saas-content', { members: [{ role: 'Lead' }, { role: 'Writer' }], works_in: 'folder' }),
      team('saas-code', { members: [{ role: 'Builder' }], works_in: 'repository' }),
    ];
    await mountWithTeams(teams, {
      runners: [DOCKET_RUNNER], fleets: [],
      agentProfiles: [teamProfile('p-1', 'Content team', 'saas-content'), teamProfile('p-2', 'Code team', 'saas-code'), teamProfile('p-3', 'Code reviewers', 'saas-code')],
    });
    const picker = document.querySelector('[data-testid="who-does-it"]')!;
    const code = picker.querySelector('[data-testid="who-team-saas-code"]')!;
    // One radio per profile, under the team that names it.
    expect([...code.querySelectorAll('input[type="radio"]')].length).toBe(2);
    expect([...code.querySelectorAll('label')].map((l) => l.textContent)).toEqual(['Code team', 'Code reviewers']);
    expect(picker.querySelector('[data-testid="who-team-saas-content"]')!.querySelectorAll('input[type="radio"]').length).toBe(1);
    expect([...picker.querySelectorAll('[data-testid="who-team-saas-content"] [aria-label="Made of"] li')].map((li) => li.textContent)).toEqual(['Lead', 'Writer']);
    // Where each works is an icon with its words.
    expect(picker.querySelector('[data-testid="who-team-saas-content"] [data-works-in="folder"]')!.getAttribute('aria-label')).toBe('Works in its own folder, not in the repository');
    expect(picker.querySelector('[data-testid="who-team-saas-code"] [data-works-in="repository"]')!.getAttribute('title')).toBe('Works on a branch of the repository');
    // What the agent can do is said once for all the teams, as five icons, not in each.
    expect(code.querySelectorAll('[data-capability]').length).toBe(0);
    expect(picker.querySelectorAll('[data-capability]').length).toBe(5);
    expect(picker.querySelectorAll('[data-testid="who-can"]').length).toBe(1);
    expect(picker.textContent?.split('Docket can stop a run, pick a run back up after a restart, answer questions and report usage.').length).toBe(2);
  });

  it('draws only a capability the agent lacks struck through, with its reason in the tooltip, and keeps a supported one’s note there', async () => {
    await mountWithTeams([team('saas-code')], { runners: [DOCKET_RUNNER], fleets: [], agentProfiles: [teamProfile('p-2', 'Code team', 'saas-code')] });
    const icon = (id: string) => document.querySelector<HTMLElement>(`[data-testid="who-can"] [data-capability="${id}"]`)!;
    const struck = (id: string) => icon(id).querySelector('svg path[d="M2.5 13.5L13.5 2.5"]') !== null;
    expect(['cancel', 'resume', 'decisions', 'proposals', 'usage'].filter(struck)).toEqual(['proposals']);
    expect(icon('proposals').title).toBe('Cannot propose items · No record set named tack_proposals');
    expect(icon('resume').title).toBe('Can pick a run back up after a restart · by task id');
  });

  it('collapses the teams no profile names into one disabled row that links to Agents', async () => {
    const teams = [team('a-team'), team('b-team'), team('c-team'), team('saas-code')];
    await mountWithTeams(teams, { runners: [DOCKET_RUNNER], fleets: [], agentProfiles: [teamProfile('p-2', 'Code team', 'saas-code')] });
    const rows = document.querySelectorAll('[data-testid="who-teams-without-profile"]');
    expect(rows.length).toBe(1);
    const row = rows[0];
    expect(row.textContent).toContain('3 teams have no profile yet');
    expect(row.textContent).toContain('a-team, b-team, c-team');
    expect(row.getAttribute('aria-disabled')).toBe('true');
    expect((row.querySelector('input[type="radio"]') as HTMLInputElement).disabled).toBe(true);
    expect(row.querySelector('a')!.getAttribute('href')).toBe('/agents');
    for (const name of ['a-team', 'b-team', 'c-team']) expect(document.querySelector(`[data-testid="who-team-${name}"]`)).toBeNull();
  });

  it('preselects the profile the item\'s role chose and says so under its row', async () => {
    await mountWithTeams([team('saas-code')], {
      runners: [DOCKET_RUNNER], fleets: [],
      agentProfiles: [teamProfile('p-1', 'Content team', 'saas-code'), teamProfile('p-2', 'Code team', 'saas-code')],
      project: { ...PROJECT, default_profile_id: null },
      resolved: { profile_id: 'p-2', profile_source: 'role:Implementer' },
    });
    expect(profileRadio('Code team').checked).toBe(true);
    expect(profileRadio('Content team').checked).toBe(false);
    const note = document.querySelector('[data-testid="profile-source"]')!;
    expect(note.textContent).toBe('Chosen by the role Implementer');
    // It sits with the chosen profile's row, not at the top of the dialog.
    expect(note.closest('[data-testid="who-does-it"]')).not.toBeNull();
    expect(note.parentElement!.querySelector('label')!.textContent).toBe('Code team');
  });

  it('opens on docket when a profile names a team, and keeps a single agent\'s profiles under that agent alone', async () => {
    const mine = { ...PROFILE_2, name: 'Fixer', tool_policy: { agent: 'codex' } };
    const runner = runnerRow('runner-1', 'This computer', runnerCapabilitySnapshot({
      harnesses: [
        { harness_kind: 'codex', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z', features: { cancel: { support: 'advisory', reason: 'sends SIGKILL to the process group' } } },
        { harness_kind: 'docket', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z' },
      ],
    }));
    await mountWithTeams([team('saas-code')], {
      runners: [runner], fleets: [], agentProfiles: [teamProfile('p-2', 'Code team', 'saas-code'), mine],
      project: { ...PROJECT, default_profile_id: null },
    });
    const picker = document.querySelector('[data-testid="who-does-it"]')!;
    // Docket comes first and is the default: its teams, and no single agent beside them.
    expect(agentChoices().map(([kind]) => kind)).toEqual(['docket', 'codex']);
    expect(chosenAgent()).toBe('docket');
    expect(picker.querySelector('[data-testid="who-team-saas-code"]')).not.toBeNull();
    expect(picker.textContent).not.toContain('Fixer');
    // Codex: its one profile, chosen since there is nothing else to choose, and no team.
    agentButton('codex').click();
    await flush();
    expect(picker.querySelector('[data-testid="who-team-saas-code"]')).toBeNull();
    expect(picker.querySelectorAll('input[type="radio"]').length).toBe(1);
    expect(profileRadio('Fixer').checked).toBe(true);
    expect(picker.querySelector<HTMLElement>('[data-testid="who-can"] [data-capability="cancel"]')!.title).toBe('Can stop a run · sends SIGKILL to the process group');
    // The adapter's note is not a line of its own.
    expect([...picker.querySelectorAll('p, span')].some((e) => e.children.length === 0 && e.textContent === 'sends SIGKILL to the process group')).toBe(false);
  });

  it('says what will happen in one line each: where, what is prepared, the checks, the sandbox and the network', async () => {
    const sandboxed = runnerRow('runner-7', 'Box', runnerCapabilitySnapshot({
      verify_configured: true,
      harnesses: [{ ...runnerCapabilitySnapshot().harnesses[0], decisions: { support: 'supported', reason: null }, sandbox: 'bwrap' }],
    }));
    const brief = { acceptance: [
      { id: 'a1', kind: 'command', title: 'tests pass', run: 'npm test', expect_exit: 0 },
      { id: 'a2', kind: 'command', title: 'lint passes', run: 'npm run lint', expect_exit: 0 },
      { id: 'a3', kind: 'manual', title: 'Reads well', text: 'x' },
      { id: 'a4', kind: 'manual', title: 'Looks right', text: 'y' },
    ] };
    mount({}, { runners: [sandboxed], fleets: [], brief, project: { ...PROJECT, prepare_command: 'npm ci' } });
    await flush();
    await flush();
    const section = document.querySelector('[data-testid="run-will-happen"]')!;
    expect(section.querySelector('h3')!.textContent).toBe('What will happen');
    const line = (label: string) => [...section.querySelectorAll('li')].find((li) => li.querySelector('.sr-only')?.textContent === `${label}: `)!;
    expect(line('Works').textContent).toContain('on a new branch of ~/src/app');
    expect(line('Prepares').textContent).toContain('Replaced: npm ci');
    expect(document.querySelector('[data-testid="run-dialog-checks"]')!.textContent).toBe('2 Done when · 2 agent checks · verifier on');
    expect(line('Sandbox').textContent).toContain('Sandboxed: the agent only reaches its workspace');
    expect(document.querySelector('[data-testid="run-dialog-network"]')!.textContent).toContain('Web tools off');
    // Each line has an icon that is hidden from a screen reader, which reads the word instead.
    for (const li of section.querySelectorAll('li')) {
      expect(li.querySelector('svg[aria-hidden="true"]')).not.toBeNull();
      expect(li.querySelector('.sr-only')).not.toBeNull();
    }
    // The toggles stay, and the network line follows the Web tools switch.
    const web = [...document.querySelectorAll('label')].find((l) => l.textContent?.trim() === 'Web tools')!.querySelector('input') as HTMLInputElement;
    web.click();
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-network"]')!.textContent).toContain('Web tools on');
  });

  it('says nothing is installed when the team works in a folder, and counts no Done when', async () => {
    const folderProfile = { ...PROFILE, tool_policy: { docket: { team: 'saas-content' } } };
    const teams = [{ name: 'saas-content', profile: 'lights-out', caps_at: null, fix: null, works_in: 'folder' }];
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [folderProfile], autonomy: { teams } });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-nothing-installed"]')!.textContent).toBe('Nothing is installed');
    expect(document.querySelector('[data-testid="run-dialog-checks"]')!.textContent).toBe('No Done when · no verifier on this runner');
  });

  it('keeps a sticky footer with the token limit, Cancel and Start run', async () => {
    const limits = { budgets: { tokens: 300000 }, timeout_seconds: 1800 };
    mount({}, { runners: [RUNNER], fleets: [], limits });
    await flush();
    await flush();
    const footer = document.querySelector<HTMLElement>('[data-testid="run-footer"]')!;
    expect(footer.classList.contains('sticky')).toBe(true);
    // It sticks to the dialog's own bottom edge, past the scroll area's padding, so nothing shows below it.
    expect(footer.classList.contains('-bottom-6')).toBe(true);
    expect([...footer.querySelectorAll('button')].map((b) => b.textContent)).toEqual(['Cancel', 'Start run']);
    expect(footer.querySelector('[data-testid="run-footer-limit"]')!.textContent).toBeTruthy();
    expect(footer.querySelector('[data-testid="run-start"]')!.textContent).toBe('Start run');
    // The field itself is still under Advanced for this run.
    expect(field('Token limit').value).toBe('300000');
    (footer.querySelector('button') as HTMLButtonElement).click();
    await flush();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it('a blocked run has no Start run in the footer, only Cancel and the link that fixes it', async () => {
    mount({}, { project: { ...PROJECT, code_origin: 'none', repository: null } });
    await flush();
    await flush();
    const footer = document.querySelector('[data-testid="run-footer"]')!;
    expect([...footer.querySelectorAll('button')].map((b) => b.textContent)).toEqual(['Cancel']);
    expect(footer.querySelector('a')!.textContent).toBe("Choose where this project's code is →");
  });
});
