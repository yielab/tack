import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import ProvisioningWizard from './ProvisioningWizard';

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={ProvisioningWizard} />
      </MemoryRouter>
    ),
    container
  );
  return { container, dispose };
}

const controlPlane = { id: 'cp-1', name: 'docket-1', kind: 'docket', health: 'healthy' };
const template = {
  id: 'tmpl-1',
  name: 'Software starter',
  project_type: 'software',
  orchestration: null,
};

/** Routes GETs by path, everything else 404s unless overridden. */
function mockFetch(overrides: {
  controlPlanes?: { status: number; body: unknown };
  templates?: { status: number; body: unknown };
  provision?: { status: number; body: unknown };
}) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = String(input);
    const method =
      typeof input === 'object' && input !== null && 'method' in input
        ? undefined
        : undefined;
    void method;
    if (url.includes('/control-planes')) {
      const o = overrides.controlPlanes ?? { status: 200, body: [controlPlane] };
      return Promise.resolve(new Response(JSON.stringify(o.body), { status: o.status }));
    }
    if (url.includes('/templates') && !url.includes('/provision')) {
      const o = overrides.templates ?? { status: 200, body: [template] };
      return Promise.resolve(new Response(JSON.stringify(o.body), { status: o.status }));
    }
    if (url.includes('/provision')) {
      const o = overrides.provision ?? {
        status: 200,
        body: {
          project: { id: 'proj-1', name: 'Blog API', description: null },
          provisioning: {
            status: 'linked',
            control_plane_id: 'cp-1',
            remote_project: 'blog-api',
            blueprint: 'software',
            members: [{ id: 'blog-api-lead', role: 'lead', model: 'anthropic/claude-opus-4-5' }],
            warnings: [],
          },
        },
      };
      return Promise.resolve(new Response(JSON.stringify(o.body), { status: o.status }));
    }
    return Promise.resolve(new Response('not found', { status: 404 }));
  });
}

describe('ProvisioningWizard — orchestration disabled (404, the default)', () => {
  it('shows the disabled explanation and renders no wizard controls', async () => {
    mockFetch({
      controlPlanes: { status: 404, body: { error: { status: 404, message: 'not found' } } },
    });
    const { container, dispose } = mount();
    await flush();
    await flush();
    expect(container.textContent).toContain('Orchestration is disabled');
    expect(container.querySelector('button')?.textContent ?? '').not.toMatch(/Provision/);
    dispose();
  });
});

describe('ProvisioningWizard — a non-404 failure', () => {
  it('shows a retry-able error, not the wizard', async () => {
    mockFetch({
      controlPlanes: { status: 500, body: { error: { status: 500, message: 'boom' } } },
    });
    const { container, dispose } = mount();
    await flush();
    await flush();
    expect(container.textContent).toContain('Could not check orchestration status');
    dispose();
  });
});

describe('ProvisioningWizard — enabled, no control planes registered', () => {
  it('shows the "register a control plane first" empty state', async () => {
    mockFetch({ controlPlanes: { status: 200, body: [] } });
    const { container, dispose } = mount();
    await flush();
    expect(container.textContent).toContain('No control planes registered');
    dispose();
  });
});

describe('ProvisioningWizard — full happy path', () => {
  it('walks through all four steps and shows the linked result, with a confirmation gate before submitting', async () => {
    mockFetch({});
    const { container, dispose } = mount();
    await flush();
    await flush();

    // Step 1
    expect(container.textContent).toContain('1. Project');
    const nameInput = container.querySelector('input') as HTMLInputElement;
    nameInput.value = 'Blog API';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    const templateSelect = container.querySelector('select') as HTMLSelectElement;
    templateSelect.value = 'tmpl-1';
    templateSelect.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();

    const next1 = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Next: Pod')
    ) as HTMLButtonElement;
    expect(next1.disabled).toBe(false);
    next1.click();
    await flush();

    // Step 2 — the remote project name should already be suggested from
    // the Tack project name.
    expect(container.textContent).toContain('Docket project name');
    const remoteProjectInput = Array.from(container.querySelectorAll('input')).find(
      (i) => (i as HTMLInputElement).value === 'blog-api'
    );
    expect(remoteProjectInput).toBeTruthy();

    const selects = container.querySelectorAll('select');
    const controlPlaneSelect = selects[0] as HTMLSelectElement;
    controlPlaneSelect.value = 'cp-1';
    controlPlaneSelect.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();

    const next2 = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Next: Review')
    ) as HTMLButtonElement;
    expect(next2.disabled).toBe(false);
    next2.click();
    await flush();

    // Step 3 — review, then the confirmation modal (no one-click submit).
    expect(container.textContent).toContain('blog-api');
    const provisionButton = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Provision…')
    ) as HTMLButtonElement;
    provisionButton.click();
    await flush();

    // The modal renders via a Portal (document.body), not inside `container`.
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('cannot be automatically undone');
    const confirmButton = Array.from(dialog.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Confirm & provision')
    ) as HTMLButtonElement;

    const fetchMock = globalThis.fetch as unknown as ReturnType<typeof vi.fn>;
    const callsBeforeSubmit = (fetchMock as any).mock.calls.length as number;
    confirmButton.click();
    await flush();
    await flush();

    // The provisioning POST only fires after the explicit confirm click.
    expect((fetchMock as any).mock.calls.length).toBeGreaterThan(callsBeforeSubmit);

    // Step 4 — linked result.
    expect(container.textContent).toContain('Linked');
    expect(container.textContent).toContain('blog-api');
    expect(container.textContent).toContain('lead');
    dispose();
  });
});

describe('ProvisioningWizard — pod created but the link write failed', () => {
  it('renders a warning, not a red error, naming the manual next step', async () => {
    mockFetch({
      provision: {
        status: 200,
        body: {
          project: { id: 'proj-1', name: 'Blog API', description: null },
          provisioning: {
            status: 'pod_created_link_failed',
            control_plane_id: 'cp-1',
            remote_project: 'blog-api',
            blueprint: 'software',
            members: [],
            warnings: [
              'the pod was provisioned successfully but Tack could not save the link — open this project\'s Settings → Orchestration panel and link it to control plane cp-1 / remote project "blog-api" to finish.',
            ],
          },
        },
      },
    });
    const { container, dispose } = mount();
    await flush();
    await flush();

    const nameInput = container.querySelector('input') as HTMLInputElement;
    nameInput.value = 'Blog API';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    const templateSelect = container.querySelector('select') as HTMLSelectElement;
    templateSelect.value = 'tmpl-1';
    templateSelect.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    (
      Array.from(container.querySelectorAll('button')).find((b) =>
        b.textContent?.includes('Next: Pod')
      ) as HTMLButtonElement
    ).click();
    await flush();

    const controlPlaneSelect = container.querySelectorAll('select')[0] as HTMLSelectElement;
    controlPlaneSelect.value = 'cp-1';
    controlPlaneSelect.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    (
      Array.from(container.querySelectorAll('button')).find((b) =>
        b.textContent?.includes('Next: Review')
      ) as HTMLButtonElement
    ).click();
    await flush();
    (
      Array.from(container.querySelectorAll('button')).find((b) =>
        b.textContent?.includes('Provision…')
      ) as HTMLButtonElement
    ).click();
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    (
      Array.from(dialog.querySelectorAll('button')).find((b) =>
        b.textContent?.includes('Confirm & provision')
      ) as HTMLButtonElement
    ).click();
    await flush();
    await flush();

    expect(container.textContent).toContain('link needs a manual step');
    expect(container.textContent).toContain('Settings → Orchestration');
    dispose();
  });
});
