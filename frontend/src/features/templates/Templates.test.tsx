import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import Templates from './Templates';
import { flush } from '../../test/flush';

const TEMPLATE = {
  id: 't1',
  name: 'Web product with content',
  description: 'A web product with its content team.',
  project_type: 'web',
  vocabulary: {},
  workflow: { workflow_type: 'scrum', statuses: [{ name: 'To Do', order: 0, category: 'todo' }], transitions: null },
  custom_fields: [],
  default_boards: [],
  roles: [
    { name: 'Content', color: '#f59e0b', icon: null, profile: 'Content writer' },
    { name: 'Security', color: '#ef4444', icon: null, profile: 'Security review' },
  ],
  profiles: [
    { name: 'Content writer', kind: 'custom', instructions: 'Write.', tool_policy: {}, limits: {}, fleet: null, summary: null },
    { name: 'Security review', kind: 'reviewer', instructions: 'Read.', tool_policy: {}, limits: {}, fleet: null, summary: null },
  ],
  automation: { autonomy: { level: 'L3', domains: [] }, auto_start: null },
  is_builtin: true,
  created_at: '2026-10-17T00:00:00Z',
  updated_at: '2026-10-17T00:00:00Z',
};

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
        <Route path="/" component={Templates} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

describe('Templates', () => {
  it('shows a template\'s roles, profiles and merge level, and lists the roles on hover', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      const body = url.includes('/api/templates') ? [TEMPLATE] : {};
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } }));
    });
    const { container, dispose } = mount();
    await flush();
    await flush();

    const text = container.textContent ?? '';
    expect(text).toContain('2 roles');
    expect(text).toContain('2 profiles');
    expect(text).toContain('Merges at L3');

    const card = container.querySelector<HTMLElement>('[tabindex="0"]')!;
    card.dispatchEvent(new MouseEvent('mouseenter', { bubbles: false }));
    await flush();
    const preview = container.querySelector('[data-testid="template-preview"]');
    expect(preview?.textContent).toContain('Content → Content writer');
    expect(preview?.textContent).toContain('Security → Security review');
    dispose();
  });
});
