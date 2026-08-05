import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ControlPlanesManager from './ControlPlanesManager';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mount(pollSecs = 10) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ControlPlanesManager pollSecs={pollSecs} />, container);
  return { container, dispose };
}

const PLANE = {
  id: 'cp-1',
  name: 'docket-prod',
  kind: 'docket',
  base_url: 'https://docket.example.com',
  api_version: null,
  health: 'healthy' as const,
  last_seen_at: '2026-08-05T00:00:00Z',
  consecutive_failures: 0,
  token_set: true,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};

beforeEach(() => {
  vi.useRealTimers();
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  document.body.innerHTML = '';
});

describe('ControlPlanesManager', () => {
  it('shows an empty state when no control planes are registered', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify([]), { status: 200 }));
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('No control planes registered');
  });

  it('lists a registered control plane with health badge and base URL', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify([PLANE]), { status: 200 }),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('docket-prod');
    expect(container.textContent).toContain('Healthy');
    expect(container.textContent).toContain('https://docket.example.com');
    expect(container.textContent).toContain('Token set');
  });

  it('registers a new control plane via the form and shows it immediately', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/control-planes') && method === 'GET') {
        return Promise.resolve(new Response(JSON.stringify([]), { status: 200 }));
      }
      if (url.endsWith('/api/control-planes') && method === 'POST') {
        return Promise.resolve(
          new Response(
            JSON.stringify({
              id: 'cp-new',
              name: 'new-plane',
              kind: 'docket',
              base_url: 'https://new.example.com',
              api_version: null,
              health: 'unknown',
              last_seen_at: null,
              consecutive_failures: 0,
              token_set: true,
              created_at: '2026-08-05T00:00:00Z',
              updated_at: '2026-08-05T00:00:00Z',
            }),
            { status: 200 },
          ),
        );
      }
      // First-health poll — never resolves meaningfully in this test since
      // timers are never advanced past the initial delay.
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const { container } = mount();
    await flush();

    const showFormBtn = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Register control plane'),
    )!;
    showFormBtn.click();
    await flush();

    const nameInput = container.querySelector<HTMLInputElement>('input[placeholder="docket-prod"]')!;
    const urlInput = container.querySelector<HTMLInputElement>(
      'input[placeholder="https://docket.internal.example.com"]',
    )!;
    nameInput.value = 'new-plane';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    urlInput.value = 'https://new.example.com';
    urlInput.dispatchEvent(new Event('input', { bubbles: true }));

    const submitBtn = Array.from(container.querySelectorAll('button[type="submit"]')).find((b) =>
      b.textContent?.includes('Register'),
    )!;
    submitBtn.click();
    await flush();

    expect(container.textContent).toContain('new-plane');
    const postCall = fetchMock.mock.calls.find(
      (c) => String(c[0]).endsWith('/api/control-planes') && (c[1] as RequestInit)?.method === 'POST',
    );
    expect(postCall).toBeTruthy();
    const body = JSON.parse((postCall![1] as RequestInit).body as string);
    expect(body).toEqual({ name: 'new-plane', base_url: 'https://new.example.com', kind: 'docket' });

    vi.useRealTimers();
  });

  it('"Check now" refetches a single control plane and updates its health badge', async () => {
    let getCount = 0;
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/api/control-planes')) {
        return Promise.resolve(new Response(JSON.stringify([{ ...PLANE, health: 'unknown' }]), { status: 200 }));
      }
      if (url.includes('/api/control-planes/cp-1')) {
        getCount++;
        return Promise.resolve(new Response(JSON.stringify({ ...PLANE, health: 'degraded' }), { status: 200 }));
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('Not yet connected');

    const checkBtn = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Check now'),
    )!;
    checkBtn.click();
    await flush();

    expect(getCount).toBe(1);
    expect(container.textContent).toContain('Degraded');
  });

  it('removes a control plane after confirmation', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/control-planes') && method === 'GET') {
        return Promise.resolve(new Response(JSON.stringify([PLANE]), { status: 200 }));
      }
      if (url.includes('/api/control-planes/cp-1') && method === 'DELETE') {
        return Promise.resolve(new Response(null, { status: 204 }));
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('docket-prod');

    const removeBtn = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Remove'),
    )!;
    removeBtn.click();
    await flush();

    expect(container.textContent).toContain('No control planes registered');
    expect(
      fetchMock.mock.calls.some(
        (c) => String(c[0]).includes('/api/control-planes/cp-1') && (c[1] as RequestInit)?.method === 'DELETE',
      ),
    ).toBe(true);
  });
});
