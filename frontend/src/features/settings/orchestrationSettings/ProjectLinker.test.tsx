import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import ProjectLinker from './ProjectLinker';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={ProjectLinker} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('ProjectLinker', () => {
  it('shows a "create a project" nudge when there are no projects', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/api/projects')) {
        return Promise.resolve(new Response(JSON.stringify([]), { status: 200 }));
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('No projects yet');
  });

  it('lists projects in a picker and renders LinkForm once one is selected and unlinked', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/api/projects')) {
        return Promise.resolve(
          new Response(JSON.stringify([{ id: 'proj-1', name: 'Website Revamp' }]), { status: 200 }),
        );
      }
      if (url.includes('/api/projects/proj-1/orch-link')) {
        return Promise.resolve(new Response(JSON.stringify({ linked: false, link: null }), { status: 200 }));
      }
      if (url.endsWith('/api/control-planes')) {
        return Promise.resolve(
          new Response(JSON.stringify([{ id: 'cp-1', name: 'docket-prod', kind: 'docket', health: 'healthy' }]), {
            status: 200,
          }),
        );
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const { container } = mount();
    await flush();

    const select = container.querySelector('select')!;
    expect(select.textContent).toContain('Website Revamp');
    select.value = 'proj-1';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();

    expect(container.textContent).toContain('Link this project to a control plane');
  });

  it('shows an "already linked" badge instead of the form when the project is linked', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/api/projects')) {
        return Promise.resolve(
          new Response(JSON.stringify([{ id: 'proj-1', name: 'Website Revamp' }]), { status: 200 }),
        );
      }
      if (url.includes('/api/projects/proj-1/orch-link')) {
        return Promise.resolve(
          new Response(
            JSON.stringify({
              linked: true,
              link: {
                project_id: 'proj-1',
                control_plane_id: 'cp-1',
                remote_project: 'website-revamp',
                pipeline_file: null,
                blueprint: null,
                auto_dispatch: false,
                budget_usd: null,
                created_at: '2026-01-01T00:00:00Z',
                updated_at: '2026-01-01T00:00:00Z',
              },
            }),
            { status: 200 },
          ),
        );
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const { container } = mount();
    await flush();
    const select = container.querySelector('select')!;
    select.value = 'proj-1';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();

    expect(container.textContent).toContain('Already linked');
    expect(container.textContent).toContain('website-revamp');
  });
});
