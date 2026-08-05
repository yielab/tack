import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import OrchestrationSettingsSection from './OrchestrationSettingsSection';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={OrchestrationSettingsSection} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

const DISABLED_ENV_DEFAULT = {
  enabled: false,
  source: 'env_default',
  reconciler_running: false,
  control_plane_count: 0,
  linked_project_count: 0,
  poll_secs: 10,
  approval_token_set: false,
  env_default: false,
};

function mockAll(initialSettings: Record<string, unknown>, opts: { settingsStatus?: number } = {}) {
  let current = { ...initialSettings };
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = (init as RequestInit | undefined)?.method ?? 'GET';
    if (url.endsWith('/api/settings/orchestration') && method === 'GET') {
      return Promise.resolve(new Response(JSON.stringify(current), { status: opts.settingsStatus ?? 200 }));
    }
    if (url.endsWith('/api/settings/orchestration') && method === 'PUT') {
      const body = JSON.parse((init as RequestInit).body as string);
      current = { ...current, enabled: body.enabled, source: 'database' };
      return Promise.resolve(new Response(JSON.stringify(current), { status: 200 }));
    }
    if (url.endsWith('/api/control-planes')) {
      return Promise.resolve(new Response(JSON.stringify([]), { status: 200 }));
    }
    if (url.endsWith('/api/projects')) {
      return Promise.resolve(new Response(JSON.stringify([]), { status: 200 }));
    }
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('OrchestrationSettingsSection', () => {
  it('renders the Off badge and explains the env_default source when disabled', async () => {
    mockAll(DISABLED_ENV_DEFAULT);
    const { container } = mount();
    await flush();

    expect(container.textContent).toContain('Off');
    expect(container.textContent).toContain('TACK_ORCH_ENABLE');
    expect(container.textContent).toContain('No database override has been saved yet');
  });

  it('names the real consequence of enabling — polling and dispatch that can spend money', async () => {
    mockAll(DISABLED_ENV_DEFAULT);
    const { container } = mount();
    await flush();
    expect(container.textContent).toMatch(/polling/i);
    expect(container.textContent).toMatch(/dispatch/i);
    expect(container.textContent).toMatch(/spend money/i);
  });

  it('locks steps 2 and 3 while disabled', async () => {
    mockAll(DISABLED_ENV_DEFAULT);
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('Turn orchestration on above to register a control plane.');
    expect(container.textContent).toContain('Turn orchestration on above, then register a control plane');
  });

  it('turning it on PUTs enabled:true, then unlocks steps 2/3 and shows live status', async () => {
    const fetchMock = mockAll(DISABLED_ENV_DEFAULT);
    const { container } = mount();
    await flush();

    const onBtn = Array.from(container.querySelectorAll('button')).find(
      (b) => b.textContent === 'On',
    )!;
    onBtn.click();
    await flush();

    const putCall = fetchMock.mock.calls.find(
      (c) =>
        String(c[0]).endsWith('/api/settings/orchestration') &&
        (c[1] as RequestInit)?.method === 'PUT',
    );
    expect(putCall).toBeTruthy();
    expect(JSON.parse((putCall![1] as RequestInit).body as string)).toEqual({ enabled: true });

    // Re-fetched settings now report enabled: true, source: database.
    expect(container.textContent).not.toContain('Turn orchestration on above to register a control plane.');
    expect(container.textContent).toContain('overriding the');
  });

  it('shows a distinct retry state when the settings fetch itself fails', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('boom', { status: 500 }));
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain("Couldn't load orchestration settings");
  });

  it('shows the reconciler/control-plane/linked-project counts when enabled and populated', async () => {
    mockAll({
      ...DISABLED_ENV_DEFAULT,
      enabled: true,
      source: 'database',
      reconciler_running: true,
      control_plane_count: 2,
      linked_project_count: 3,
      poll_secs: 15,
      approval_token_set: true,
    });
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('Running');
    expect(container.textContent).toContain('15s');
    expect(container.textContent).toContain('Configured');
  });
});
