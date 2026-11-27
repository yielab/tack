import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import Projects from './Projects';
import { flush } from '../../test/flush';

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mount(projects: unknown[]) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((input) =>
    Promise.resolve(new Response(JSON.stringify(String(input).includes('/api/projects') ? projects : {}), { status: 200 })),
  );
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={Projects} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

const isSolid = (el: Element | undefined) => (el?.getAttribute('style') ?? '').includes('--color-primary-600');
const buttonNamed = (c: HTMLElement, text: string) => [...c.querySelectorAll('button')].find((b) => (b.textContent ?? '').includes(text));

describe('Projects', () => {
  it('makes New Project the solid button when there are projects', async () => {
    const { container, dispose } = mount([
      { id: 'p1', name: 'My SaaS', description: '', project_type: 'software', created_at: '2026-11-01T00:00:00Z' },
    ]);
    await flush();
    await flush();
    expect(isSolid(buttonNamed(container, 'New Project'))).toBe(true);
    dispose();
  });

  it('leaves the first-project button the only solid one when there are no projects', async () => {
    const { container, dispose } = mount([]);
    await flush();
    await flush();
    expect(isSolid(buttonNamed(container, 'New Project'))).toBe(false);
    expect(isSolid(buttonNamed(container, 'Create your first project'))).toBe(true);
    dispose();
  });
});
