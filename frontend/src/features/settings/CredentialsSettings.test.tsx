import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import CredentialsSettings, { credentialState } from './CredentialsSettings';
import { flush } from '../../test/flush';

const PROJECT = { id: '11111111-1111-4111-8111-111111111111', name: 'My SaaS' };
const NOW = '2026-11-01T12:00:00Z';

const credential = (over: Record<string, unknown> = {}) => ({
  id: '22222222-2222-4222-8222-222222222222',
  name: 'Reviewer',
  scopes: [{ all_projects: false, projects: [PROJECT.id], capabilities: ['read'] }],
  created_at: NOW,
  expires_at: '2027-01-30T12:00:00Z',
  last_used_at: null,
  revoked_at: null,
  ...over,
});

let credentials: ReturnType<typeof credential>[];
let calls: { method: string; url: string; body?: string }[];

beforeEach(() => {
  credentials = [credential()];
  calls = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = init?.method ?? 'GET';
    calls.push({ method, url, body: init?.body as string | undefined });
    const json = (body: unknown) =>
      Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } }));
    if (url.endsWith('/api/agent-credentials') && method === 'GET') return json(credentials);
    if (url.endsWith('/api/agent-credentials') && method === 'POST') {
      const made = credential({ id: '33333333-3333-4333-8333-333333333333', name: 'Planner' });
      credentials = [made, ...credentials];
      return json({ credential: made, secret: 'tack_agent_secretvalue' });
    }
    if (url.includes('/rotate')) return json({ credential: credentials[0], secret: 'tack_agent_rotated' });
    if (url.includes('/revoke')) {
      credentials = credentials.map((c) => ({ ...c, revoked_at: NOW }));
      return json(credentials[0]);
    }
    if (url.endsWith('/api/projects')) return json([PROJECT]);
    return json({});
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <CredentialsSettings />, container);
  return { container, dispose };
}

const button = (c: HTMLElement, text: string) =>
  Array.from(c.querySelectorAll('button')).find((b) => b.textContent?.trim() === text)!;

describe('CredentialsSettings', () => {
  it('lists each credential with its scope in words, its state and its dates', async () => {
    const { container } = mount();
    await flush();
    const row = container.querySelector('[data-testid="credential-list"] li')!;
    expect(row.textContent).toContain('Reviewer');
    expect(row.textContent).toContain('active');
    expect(row.textContent).toContain('My SaaS: Read');
    expect(row.textContent).toContain('Never used');
  });

  it('shows the issue button disabled with its reason until the form is filled', async () => {
    const { container } = mount();
    await flush();
    // Issuing needs a name and a project first.
    const issue = button(container, 'Issue credential');
    expect(issue.disabled).toBe(true);
    expect(container.textContent).toContain('Give the credential a name');
    // Every capability can be picked: change sets and threads exist.
    expect(container.querySelector('input[aria-describedby="cap-propose-reason"]')).toBeNull();
    expect(container.querySelector('input[aria-describedby="cap-converse-reason"]')).toBeNull();
  });

  it('issues a credential and shows its secret once, with a copy button', async () => {
    const { container } = mount();
    await flush();
    const name = container.querySelector<HTMLInputElement>('input[placeholder="Reviewer"]')!;
    name.value = 'Planner';
    name.dispatchEvent(new InputEvent('input', { bubbles: true }));
    const projectBox = Array.from(container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')).find((b) =>
      b.closest('label')?.textContent?.includes('My SaaS'),
    )!;
    projectBox.checked = true;
    projectBox.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    const issue = button(container, 'Issue credential');
    expect(issue.disabled).toBe(false);
    issue.click();
    await flush();
    const post = calls.find((c) => c.method === 'POST' && c.url.endsWith('/api/agent-credentials'))!;
    expect(JSON.parse(post.body!)).toEqual({
      name: 'Planner',
      scopes: [{ all_projects: false, projects: [PROJECT.id], capabilities: ['read'] }],
    });
    const secret = container.querySelector('[data-testid="credential-secret"]')!;
    expect(secret.textContent).toContain('tack_agent_secretvalue');
    expect(secret.textContent).toContain('only time it is shown');
    expect(button(container, 'Copy')).toBeTruthy();
    button(container, 'I have copied it').click();
    await flush();
    expect(container.querySelector('[data-testid="credential-secret"]')).toBeNull();
  });

  it('disables Rotate and Revoke for a revoked credential and says why', async () => {
    credentials = [credential({ revoked_at: NOW })];
    const { container } = mount();
    await flush();
    expect(button(container, 'Rotate').disabled).toBe(true);
    expect(button(container, 'Revoke').disabled).toBe(true);
    expect(container.textContent).toContain('Already revoked');
  });

  it('names a credential expired once its date has passed', () => {
    expect(credentialState(credential() as never, Date.parse('2026-12-01T00:00:00Z'))).toBe('active');
    expect(credentialState(credential() as never, Date.parse('2027-02-01T00:00:00Z'))).toBe('expired');
    expect(credentialState(credential({ revoked_at: NOW }) as never)).toBe('revoked');
  });
});
