import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { createSignal } from 'solid-js';
import { MemoryRouter, Route } from '@solidjs/router';
import { ExecutionStoreProvider } from '../state/executionContext';
import RunWithAgentModal from './RunWithAgentModal';
import type { RunnerCapabilities } from '../execution';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

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

function Host(props: { capabilities?: () => RunnerCapabilities[]; onCreated?: (id: string) => void }) {
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
            />
          )}
        />
      </MemoryRouter>
    </ExecutionStoreProvider>
  );
}

function mount(
  props: { capabilities?: () => RunnerCapabilities[]; onCreated?: (id: string) => void } = {},
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
  return [...document.querySelectorAll('button')].find((b) => b.textContent === 'Run') as HTMLButtonElement;
}

function teardown() {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
}

const modelOptions = () => [...select('Model').options].map((o) => o.textContent);

afterEach(() => {
  teardown();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  lastCreateBody = undefined;
  lastItemPatch = undefined;
  requestedUrls = [];
});

describe('RunWithAgentModal', () => {
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
    await new Promise((r) => setTimeout(r, 450));
    await flush();
    expect(sentence()).toBe('Server says ?profile_id=profile-1&harness=codex&tokens=120000');
    // The agent chosen goes up too.
    setSelect(select('Agent'), 'claude-code');
    await flush();
    await flush();
    expect(sentence()).toBe('Server says ?profile_id=profile-1&harness=claude-code&tokens=120000');
    // A cleared field says "no limit".
    setField(field('Token limit'), '');
    await new Promise((r) => setTimeout(r, 450));
    await flush();
    expect(sentence()).toBe('Server says ?profile_id=profile-1&harness=claude-code&no_limit=true');
    expect(limitUrls().every((u) => u.startsWith('/api/items/item-1/agent-context?profile_id=profile-1&harness='))).toBe(true);
    expect(textUrls().length).toBe(textFetches);
  });

  it('prints_no_limit_sentence_while_the_token_field_holds_an_invalid_value', async () => {
    mount({}, { runners: [RUNNER], fleets: [], limits: { budgets: { tokens: 300000 } } });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-limit"]')).not.toBeNull();
    const before = requestedUrls.length;
    setField(field('Token limit'), '0');
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-limit"]')).toBeNull();
    await new Promise((r) => setTimeout(r, 450));
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-limit"]')).toBeNull();
    expect(document.body.textContent).toContain('Token limit must be a whole number above zero');
    // The server was never asked about a value it would refuse.
    expect(requestedUrls.slice(before).some((u) => u.includes('tokens=0'))).toBe(false);
    setField(field('Token limit'), '5000');
    await new Promise((r) => setTimeout(r, 450));
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
    const pill = Array.from(document.querySelectorAll('[role="group"][aria-label="Profile"] button')).find((b) => b.textContent === 'Fixer') as HTMLButtonElement;
    expect(pill.getAttribute('aria-pressed')).toBe('true');
    expect(document.querySelector('[data-testid="profile-source"]')?.textContent).toBe('Chosen by the role Content');
    // A profile the person picks instead is theirs, not the server's.
    (Array.from(document.querySelectorAll('[role="group"][aria-label="Profile"] button')).find((b) => b.textContent === 'Reviewer') as HTMLButtonElement).click();
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
    const pill = Array.from(document.querySelectorAll('[role="group"][aria-label="Profile"] button')).find((b) => b.textContent === 'Reviewer') as HTMLButtonElement;
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

  it('one answering runner and no group is one fixed line, "Runs on this computer" for this computer', async () => {
    mount({}, { runners: [{ ...RUNNER, name: 'This computer (dev)' }], fleets: [] });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="run-target-fixed"]')?.textContent).toBe('Runs on this computer');
    expect(document.querySelector('label[for]')?.textContent).not.toContain('Machine or group');
  });

  it('a profile pick picks its agent and says so, only while that choice stands', async () => {
    const both = runnerRow('runner-9', 'Two agents', runnerCapabilitySnapshot({
      harnesses: ['codex', 'claude-code'].map((harness_kind) => ({
        harness_kind, installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z', requires_model: false,
      })),
    }));
    const claudeProfile = { ...PROFILE_2, tool_policy: { tools: { 'claude-code': ['Read'] } } };
    mount({}, {
      runners: [both], fleets: [], agentProfiles: [PROFILE, claudeProfile],
      project: { ...PROJECT, default_profile_id: 'profile-1' }, runSettings: { harness: 'codex' },
    });
    await flush();
    await flush();
    const notice = () => document.querySelector('[data-testid="profile-agent-notice"]')?.textContent;
    expect(select('Agent').value).toBe('codex');
    expect(notice()).toBeUndefined();
    const pill = (name: string) => Array.from(document.querySelectorAll('[role="group"][aria-label="Profile"] button')).find((b) => b.textContent === name) as HTMLButtonElement;
    pill('Fixer').click();
    await flush();
    expect(select('Agent').value).toBe('claude-code');
    expect(notice()).toContain('Fixer runs on Claude Code');
    // Choosing the agent by hand afterwards ends the notice.
    setSelect(select('Agent'), 'codex');
    await flush();
    expect(notice()).toBeUndefined();
  });

  it('lists the profiles the person made first, each with what it delivers, and the built-in ones apart', async () => {
    const mine = { ...PROFILE_2, name: 'Fixer', delivers: 'a change on a branch' };
    const builtIn = { ...PROFILE, name: 'Reviewer', builtin: true, kind: 'reviewer' };
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [builtIn, mine] });
    await flush();
    await flush();
    const group = document.querySelector('[role="group"][aria-label="Profile"]')!;
    const names = [...group.querySelectorAll('button')].map((b) => b.textContent);
    expect(names).toEqual(['Fixer', 'Reviewer']);
    expect(group.textContent).toContain('Your profiles');
    expect(group.textContent).toContain('Built in');
    expect(group.textContent).toContain('a change on a branch');
  });

  it('a profile that runs no commands says nothing is installed', async () => {
    const reader = { ...PROFILE, runs_commands: false };
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [reader] });
    await flush();
    await flush();
    expect(document.querySelector('[data-testid="run-dialog-install"]')?.textContent).toBe('No install: this profile runs no commands');
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
    const pill = [...dialog.querySelectorAll('[aria-label="Profile"] button')].find((b) => b.textContent === 'Default')!;
    expect(pill.getAttribute('aria-pressed')).toBe('true');
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
    const researcher = { ...PROFILE, agent_profile_id: 'profile-net', name: 'Researcher', tool_policy: { network: true } };
    const web = () => ([...document.querySelectorAll('label')].find((l) => l.textContent?.trim() === 'Web tools')!.querySelector('input') as HTMLInputElement);
    const pick = (name: string) => (Array.from(document.querySelectorAll('[role="group"][aria-label="Profile"] button')).find((b) => b.textContent === name) as HTMLButtonElement).click();
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
    setSelect(select('Agent'), 'claude-code');
    await flush();
    expect(document.querySelectorAll('[aria-label="Suggested tools"] input').length).toBe(8);
    expect(field('Allowed tools')).toBeTruthy();
  });

  it('with only claude-code reported, the harness is claude-code — what the select shows is what is sent', async () => {
    const claudeOnly = runnerRow('runner-6', 'Claude only', runnerCapabilitySnapshot({
      harnesses: [{ ...runnerCapabilitySnapshot().harnesses[0], harness_kind: 'claude-code' }],
    }));
    mount({}, { runners: [claudeOnly], fleets: [] });
    await flush();
    expect(select('Agent').value).toBe('claude-code');
    expect(document.body.textContent).not.toContain('Codex exposes no list');
  });

  it('a docket-only runner offers docket under its label, so a docket runner can be run from the dialog', async () => {
    const docketOnly = runnerRow('runner-7', 'Docket only', runnerCapabilitySnapshot({
      harnesses: [{ ...runnerCapabilitySnapshot().harnesses[0], harness_kind: 'docket' }],
    }));
    mount({}, { runners: [docketOnly], fleets: [] });
    await flush();
    const options = [...select('Agent').options].map((o) => [o.value, o.textContent]);
    expect(options).toEqual([['docket', 'Docket']]);
    expect(select('Agent').value).toBe('docket');
    expect(document.body.textContent).toContain('Docket exposes no list');
  });

  it('a reported harness outside HARNESS_KINDS is offered under its wire name', async () => {
    const unknownOnly = runnerRow('runner-8', 'Unknown only', runnerCapabilitySnapshot({
      harnesses: [{ ...runnerCapabilitySnapshot().harnesses[0], harness_kind: 'prill' }],
    }));
    mount({}, { runners: [unknownOnly], fleets: [] });
    await flush();
    const options = [...select('Agent').options].map((o) => [o.value, o.textContent]);
    expect(options).toEqual([['prill', 'prill']]);
    expect(select('Agent').value).toBe('prill');
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
