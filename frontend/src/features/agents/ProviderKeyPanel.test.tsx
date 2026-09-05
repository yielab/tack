import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ProviderKeyPanel from './ProviderKeyPanel';
import { VERCEL_AI_GATEWAY_SECRET_NAME } from './api';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ProviderKeyPanel />, container);
  return { container, dispose };
}

interface Backend {
  status: 200 | 404 | 500;
  catalog: Record<string, unknown>;
  secrets: { name: string; set_at: string | null }[];
}

function mockFetch(backend: Backend) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = (init as RequestInit | undefined)?.method ?? 'GET';

    if (url.endsWith('/api/local-runner') && method === 'GET') {
      return Promise.resolve(
        new Response(
          JSON.stringify({ enabled: false, state: 'stopped', since: null, catalog: backend.catalog }),
          { status: backend.status },
        ),
      );
    }
    if (url.endsWith('/api/local-runner/secrets') && method === 'GET') {
      return Promise.resolve(
        new Response(JSON.stringify({ data: backend.secrets }), { status: backend.status }),
      );
    }
    if (url.includes('/api/local-runner/secrets/') && method === 'PUT') {
      const body = JSON.parse((init as RequestInit).body as string) as { value: string };
      expect(body.value).toBeTruthy();
      backend.secrets = [
        { name: VERCEL_AI_GATEWAY_SECRET_NAME, set_at: new Date().toISOString() },
      ];
      backend.catalog = { status: 'configured', model_count: 7, checked_at: new Date().toISOString() };
      return Promise.resolve(new Response(null, { status: 204 }));
    }
    if (url.includes('/api/local-runner/secrets/') && method === 'DELETE') {
      backend.secrets = [];
      backend.catalog = { status: 'not_configured' };
      return Promise.resolve(new Response(null, { status: 204 }));
    }
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('ProviderKeyPanel', () => {
  it('renders the write-only field and a not-configured catalog line when no key is set', async () => {
    mockFetch({ status: 200, catalog: { status: 'not_configured' }, secrets: [] });
    const { container } = mount();
    await flush();

    expect(container.querySelector('input[type="password"]')).toBeTruthy();
    expect(container.textContent).toContain('Catalog: not configured');
  });

  it('never renders the pasted value anywhere in the DOM after save', async () => {
    mockFetch({ status: 200, catalog: { status: 'not_configured' }, secrets: [] });
    const { container } = mount();
    await flush();

    const input = container.querySelector('input[type="password"]') as HTMLInputElement;
    const secretValue = 'sk-live-do-not-leak-this';
    input.value = secretValue;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    const form = container.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await flush();
    await flush();
    await flush();

    expect(container.innerHTML).not.toContain(secretValue);
    expect(container.textContent).toContain('Set ');
    expect(container.textContent).toContain('Catalog: 7 models as of');
  });

  it('shows Set/Replace/Remove once a key is stored', async () => {
    mockFetch({
      status: 200,
      catalog: { status: 'configured', model_count: 3, checked_at: new Date().toISOString() },
      secrets: [{ name: VERCEL_AI_GATEWAY_SECRET_NAME, set_at: new Date().toISOString() }],
    });
    const { container } = mount();
    await flush();

    expect(container.textContent).toContain('Set ');
    expect(container.textContent).toContain('Replace');
    expect(container.textContent).toContain('Remove');
    expect(container.querySelector('input[type="password"]')).toBeFalsy();
  });

  it('renders the console fallback on a genuine 404', async () => {
    mockFetch({ status: 404, catalog: { status: 'not_configured' }, secrets: [] });
    const { container } = mount();
    await flush();

    expect(container.textContent).toContain('tack runner secret set');
    expect(container.textContent).toContain(VERCEL_AI_GATEWAY_SECRET_NAME);
    expect(container.textContent).not.toContain('Catalog:');
  });
});
