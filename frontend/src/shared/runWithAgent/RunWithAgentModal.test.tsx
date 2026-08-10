import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { createSignal } from 'solid-js';
import { ExecutionStoreProvider } from '../state/executionContext';
import RunWithAgentModal from './RunWithAgentModal';
import type { RunnerCapabilities } from '../execution';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

const FLEET = { fleet_id: 'fleet-1', name: 'Primary Fleet', concurrency_limit: 5, default_policy: null };
const PROFILE = { agent_profile_id: 'profile-1', name: 'Reviewer', instructions: 'Review the diff', tool_policy: { read: true }, limits: null };
const MODEL_ENABLED = { model_profile_id: 'mp-1', name: 'GPT alpha', model_provider: 'openai', model_id: 'opaque/model-alpha', config_reference: null, enabled: true };
const MODEL_DISABLED = { model_profile_id: 'mp-2', name: 'Retired', model_provider: 'openai', model_id: 'opaque/model-old', config_reference: null, enabled: false };

let lastCreateBody: unknown;

function mockFetch(): typeof fetch {
  return (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.includes('/runner-fleets')) return new Response(JSON.stringify({ protocol_version: 1, data: [FLEET] }), { status: 200 });
    if (url.includes('/agent-profiles')) return new Response(JSON.stringify({ protocol_version: 1, data: [PROFILE] }), { status: 200 });
    if (url.includes('/model-profiles')) return new Response(JSON.stringify({ protocol_version: 1, data: [MODEL_ENABLED, MODEL_DISABLED] }), { status: 200 });
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
      <RunWithAgentModal
        isOpen={open()}
        onClose={() => setOpen(false)}
        itemId="item-1"
        itemTitle="Fix login bug"
        onCreated={props.onCreated}
        capabilities={props.capabilities}
      />
    </ExecutionStoreProvider>
  );
}

function mount(props: { capabilities?: () => RunnerCapabilities[]; onCreated?: (id: string) => void } = {}) {
  vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch());
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
}

function setField(el: HTMLInputElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

function submitButton(): HTMLButtonElement {
  return [...document.querySelectorAll('button')].find((b) => b.textContent === 'Run') as HTMLButtonElement;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  lastCreateBody = undefined;
});

describe('RunWithAgentModal', () => {
  it('renders with the item name in the title, and every section labeled', async () => {
    mount();
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Run with agent: Fix login bug');
    expect(dialog.textContent).toContain('Target');
    expect(dialog.textContent).toContain('Repository');
    expect(dialog.textContent).toContain('Permissions & budget');
  });

  it('submit is disabled with every missing-field reason listed when the form is empty', async () => {
    mount();
    await flush();
    expect(submitButton().disabled).toBe(true);
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Select a fleet.');
    expect(dialog.textContent).toContain('Select an agent profile.');
    expect(dialog.textContent).toContain('Enter a repository remote.');
  });

  it('with zero injected capabilities and Auto model mode, filling the required fields enables submit', async () => {
    mount();
    await flush();
    setSelect(select('Fleet'), FLEET.fleet_id);
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    setField(field('Remote'), 'git@example.com:org/repo.git');
    await flush();
    expect(submitButton().disabled).toBe(false);
  });

  it('choosing "Choose a model" without picking one blocks submit with a named reason', async () => {
    mount();
    await flush();
    setSelect(select('Fleet'), FLEET.fleet_id);
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    setField(field('Remote'), 'git@example.com:org/repo.git');
    const modelModeRadio = [...document.querySelectorAll('input[type="radio"][name="model-mode"]')][1] as HTMLInputElement;
    modelModeRadio.click();
    await flush();
    expect(submitButton().disabled).toBe(true);
    expect(document.body.textContent).toContain('Select a model, or switch to Auto.');
  });

  it('a specific model with zero capability data blocks submit and shows "Unsupported"', async () => {
    mount();
    await flush();
    setSelect(select('Fleet'), FLEET.fleet_id);
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    setField(field('Remote'), 'git@example.com:org/repo.git');
    const modelModeRadio = [...document.querySelectorAll('input[type="radio"][name="model-mode"]')][1] as HTMLInputElement;
    modelModeRadio.click();
    await flush();
    setSelect(select('Model'), MODEL_ENABLED.model_profile_id);
    await flush();
    expect(submitButton().disabled).toBe(true);
    expect(document.body.textContent).toContain('Unsupported');
  });

  it('only enabled model profiles are offered — a disabled one never appears as an option', async () => {
    mount();
    await flush();
    const modelModeRadio = [...document.querySelectorAll('input[type="radio"][name="model-mode"]')][1] as HTMLInputElement;
    modelModeRadio.click();
    await flush();
    const modelSelect = select('Model');
    const optionValues = [...modelSelect.options].map((o) => o.value);
    expect(optionValues).toContain(MODEL_ENABLED.model_profile_id);
    expect(optionValues).not.toContain(MODEL_DISABLED.model_profile_id);
  });

  it('proves the gate is load-bearing: injecting a real supported combination allows submit', async () => {
    const caps: RunnerCapabilities[] = [
      {
        runner_version: '0.1.0',
        reported_at: '2026-08-06T12:00:00Z',
        labels: {},
        concurrency: { total: 1, available: 1 },
        harnesses: [
          {
            harness_kind: 'codex',
            installed_version: '1.0.0',
            probe_error: null,
            probed_at: '2026-08-06T12:00:00Z',
            model_combinations: [{ model_provider: 'openai', model_ids: ['opaque/model-alpha'], discovery: 'reported' }],
          },
        ],
        features: {
          cancel: { support: 'advisory', reason: null },
          resume: { support: 'unsupported', reason: null },
          decisions: { support: 'supported', reason: null },
          artifacts: { support: 'supported', reason: null },
          usage: { support: 'advisory', reason: null },
        },
        limits: { event_payload_bytes_max: 1024, artifact_content_bytes_max: 1024 },
      },
    ];
    mount({ capabilities: () => caps });
    await flush();
    setSelect(select('Fleet'), FLEET.fleet_id);
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    setField(field('Remote'), 'git@example.com:org/repo.git');
    const modelModeRadio = [...document.querySelectorAll('input[type="radio"][name="model-mode"]')][1] as HTMLInputElement;
    modelModeRadio.click();
    await flush();
    setSelect(select('Model'), MODEL_ENABLED.model_profile_id);
    await flush();
    expect(submitButton().disabled).toBe(false);
    expect(document.body.textContent).toContain('Supported');
  });

  it('submitting posts the exact CreateExecutionInput shape, closes, and calls onCreated', async () => {
    const onCreated = vi.fn();
    mount({ onCreated });
    await flush();
    setSelect(select('Fleet'), FLEET.fleet_id);
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    setField(field('Remote'), 'git@example.com:org/repo.git');
    await flush();
    expect(submitButton().disabled).toBe(false);
    submitButton().click();
    await flush();
    await flush();

    expect(lastCreateBody).toMatchObject({
      item_id: 'item-1',
      selector_kind: 'fleet',
      selector_id: 'fleet-1',
      agent_profile_id: 'profile-1',
      requested_harness_kind: 'codex',
      requested_model_provider: null,
      requested_model_id: null,
      repository_snapshot: { kind: 'git', remote: 'git@example.com:org/repo.git', base_revision: 'main', subdirectory: null },
      permission_policy: { tools: [], network: false },
    });
    expect(typeof (lastCreateBody as Record<string, unknown>).idempotency_key).toBe('string');
    expect(onCreated).toHaveBeenCalledWith('req-9');
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it('every field has an accessible label (native <label for>) — the keyboard/a11y path this card requires', async () => {
    mount();
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
