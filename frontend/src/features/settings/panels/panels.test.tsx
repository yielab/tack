import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../../shared/state/projectContext';
import type { Resource } from 'solid-js';
import type { Project } from '../../../shared/types';
import GeneralPanel from './GeneralPanel';
import RolesPanel from './RolesPanel';
import DataPanel from './DataPanel';

const PROJECT = {
  id: 'p1',
  name: 'Project One',
  description: 'desc',
  project_type: 'software',
  vocabulary: {},
  workflow: { workflow_type: 'kanban', statuses: [] },
  archived: false,
} as unknown as Project;

const ctx: ProjectContextValue = {
  projectId: () => 'p1',
  project: (() => PROJECT) as unknown as Resource<Project>,
  workflow: () => PROJECT.workflow,
  vocabulary: () => ({}),
  refetch: () => {},
};

const flush = () => new Promise((r) => setTimeout(r, 0));
let fetchMock: ReturnType<typeof vi.spyOn>;

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

beforeEach(() => {
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:x');
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = String(input);
    if (url.includes('/export')) return Promise.resolve(new Response(new Blob(['x']), { status: 200 }));
    if (url.endsWith('/api/projects/p1/roles')) return Promise.resolve(new Response('[]', { status: 200 }));
    if (url.endsWith('/api/projects/import')) return Promise.resolve(new Response(JSON.stringify({ id: 'new' }), { status: 200 }));
    return Promise.resolve(new Response(JSON.stringify({ id: 'p1' }), { status: 200 }));
  });
});

function mount(comp: () => unknown) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={ctx}>
        <MemoryRouter>
          <Route path="/" component={() => comp() as never} />
        </MemoryRouter>
      </ProjectContext.Provider>
    ),
    container,
  );
  return { container, dispose };
}

const btn = (c: HTMLElement, text: string) =>
  Array.from(c.querySelectorAll('button')).find((b) => b.textContent?.trim() === text)!;

describe('Settings panels', () => {
  it('GeneralPanel saves via PATCH /projects/{id}', async () => {
    const { container } = mount(() => <GeneralPanel />);
    await flush();
    btn(container, 'Save').click();
    await flush();
    const patch = fetchMock.mock.calls.find(
      (c) => (c[1] as RequestInit)?.method === 'PATCH' && String(c[0]).endsWith('/api/projects/p1'),
    );
    expect(patch).toBeTruthy();
    expect(JSON.parse((patch![1] as RequestInit).body as string)).toMatchObject({ name: 'Project One' });
  });

  it('RolesPanel adds a role via POST /projects/{id}/roles', async () => {
    const { container } = mount(() => <RolesPanel />);
    await flush();
    const nameInput = container.querySelector<HTMLInputElement>('input:not([type="color"])')!;
    nameInput.value = 'Designer';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    container.querySelector('form')!.dispatchEvent(new Event('submit'));
    await flush();
    expect(
      fetchMock.mock.calls.some(
        (c) => (c[1] as RequestInit)?.method === 'POST' && String(c[0]).endsWith('/api/projects/p1/roles'),
      ),
    ).toBe(true);
  });

  it('DataPanel exports (GET /export) and imports (POST /projects/import)', async () => {
    const { container } = mount(() => <DataPanel />);
    await flush();

    btn(container, 'Export JSON').click();
    await flush();
    expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/api/projects/p1/export?format=json'))).toBe(true);
    expect(URL.createObjectURL).toHaveBeenCalled();

    const input = container.querySelector<HTMLInputElement>('input[type="file"]')!;
    const file = new File([JSON.stringify({ project: {}, items: [] })], 'p.json', { type: 'application/json' });
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    input.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect(
      fetchMock.mock.calls.some(
        (c) => (c[1] as RequestInit)?.method === 'POST' && String(c[0]).endsWith('/api/projects/import'),
      ),
    ).toBe(true);
  });
});
