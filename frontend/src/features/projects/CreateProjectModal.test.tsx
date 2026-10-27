import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import CreateProjectModal from './CreateProjectModal';

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

/** What `check-folder` answers for the one folder these tests type. */
let folderCheck = { exists: true, is_dir: true, is_git: true, branch: 'main', remote_url: null, dirty_files: 0 };

function mockFetch() {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = init?.method ?? 'GET';
    if (url.endsWith('/api/templates')) return Promise.resolve(new Response('[]', { status: 200 }));
    if (url.endsWith('/api/local-runner/check-folder')) {
      return Promise.resolve(new Response(JSON.stringify(folderCheck), { status: 200 }));
    }
    if (url.includes('/api/projects/outline') && method === 'POST') {
      const result = { dry_run: url.includes('dry_run'), created: 2, skipped: 0, dependencies: 0, warnings: [], items: [], sections: [], project: url.includes('dry_run') ? null : { id: 'proj-9', name: 'From file' } };
      return Promise.resolve(new Response(JSON.stringify(result), { status: 200 }));
    }
    if (url.endsWith('/api/projects') && method === 'POST') {
      return Promise.resolve(new Response(JSON.stringify({ id: 'proj-1', name: 'New' }), { status: 200 }));
    }
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
}

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={() => <CreateProjectModal isOpen onClose={() => {}} onSuccess={() => {}} />} />
      </MemoryRouter>
    ),
    container,
  );
  return { dispose };
}

/** The modal renders through a portal, so every lookup is on the document. */
const dialog = () => document.querySelector<HTMLElement>('[role="dialog"]')!;
const input = (selector: string) => dialog().querySelector<HTMLInputElement>(selector)!;
const radio = (label: string) =>
  Array.from(dialog().querySelectorAll('label')).find((l) => l.textContent?.trim() === label)!.querySelector('input')!;

function typeInto(el: HTMLInputElement, value: string, event: 'input' | 'change') {
  el.value = value;
  el.dispatchEvent(new Event(event, { bubbles: true }));
}

/** Every request to `/api/projects` or one of its rows, as `[method, path, body]`. */
function projectRequests(fetchSpy: ReturnType<typeof mockFetch>) {
  return fetchSpy.mock.calls
    .filter(([u]) => /\/api\/projects(\/|$)/.test(String(u)))
    .map(([u, init]) => [init?.method ?? 'GET', String(u).replace(/^.*\/api/, '/api'), init?.body ? JSON.parse(String(init.body)) : null]);
}

async function createWithFolder(fetchSpy: ReturnType<typeof mockFetch>) {
  mount();
  await flush();
  typeInto(input('input[placeholder="My Awesome Project"]'), 'New', 'input');
  radio('An existing folder on this computer').click();
  await flush();
  typeInto(input('input[aria-label="Code folder"]'), '/home/me/code', 'change');
  await flush();
  await flush();
  dialog().querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await flush();
  await flush();
  return projectRequests(fetchSpy);
}

describe('CreateProjectModal', () => {
  it('shows Browse… disabled with its reason in a browser tab, and enabled in the desktop app', async () => {
    mockFetch();
    mount();
    await flush();
    radio('An existing folder on this computer').click();
    await flush();
    const browse = () => Array.from(dialog().querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Browse…')!;
    expect(browse().disabled).toBe(true);
    expect(dialog().querySelector(`#${CSS.escape(browse().getAttribute('aria-describedby')!)}`)!.textContent).toBe('Available in the desktop app');

    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    try {
      document.body.innerHTML = '';
      mount();
      await flush();
      radio('An existing folder on this computer').click();
      await flush();
      expect(browse().disabled).toBe(false);
    } finally {
      delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    }
  });

  it('creates a project with its folder in the one POST, with no PATCH afterwards', async () => {
    folderCheck = { ...folderCheck, is_git: true, branch: 'main' };
    const requests = await createWithFolder(mockFetch());
    expect(requests).toEqual([
      ['POST', '/api/projects', {
        name: 'New',
        project_type: 'software',
        code_origin: 'folder',
        repository: '/home/me/code',
        workspace_mode: 'local_branch',
        default_branch: 'main',
      }],
    ]);
  });

  it('a folder without git is created to be worked in directly, still in the one POST', async () => {
    folderCheck = { ...folderCheck, is_git: false, branch: null as unknown as string };
    const requests = await createWithFolder(mockFetch());
    expect(requests).toHaveLength(1);
    expect(requests[0][2]).toMatchObject({ code_origin: 'folder', repository: '/home/me/code', workspace_mode: 'in_place', default_branch: null });
  });

  it('a project with no code is created with code_origin none in the one POST', async () => {
    const fetchSpy = mockFetch();
    mount();
    await flush();
    typeInto(input('input[placeholder="My Awesome Project"]'), 'New', 'input');
    dialog().querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await flush();
    await flush();
    const requests = projectRequests(fetchSpy);
    expect(requests).toHaveLength(1);
    expect(requests[0][0]).toBe('POST');
    expect(requests[0][2]).toMatchObject({ code_origin: 'none', repository: null, workspace_mode: 'local_branch', default_branch: null });
  });

  it('a project file is previewed, then creates the project with one confirm', async () => {
    const fetchSpy = mockFetch();
    mount();
    await flush();
    const choose = Array.from(dialog().querySelectorAll('button')).find((b) => b.textContent?.trim() === 'From a project file…');
    expect(choose, 'the modal offers "From a project file"').toBeTruthy();
    const fileInput = dialog().querySelector<HTMLInputElement>('input[type="file"]')!;
    const file = new File(['name: From file\nitems: []'], 'bundle.yaml', { type: 'application/x-yaml' });
    Object.defineProperty(fileInput, 'files', { value: [file], configurable: true });
    fileInput.dispatchEvent(new Event('change', { bubbles: true }));

    const outlinePosts = () =>
      fetchSpy.mock.calls
        .filter(([u, init]) => init?.method === 'POST' && String(u).includes('/api/projects/outline'))
        .map(([u]) => String(u).replace(/^.*\/api/, '/api'));
    await vi.waitFor(() => expect(outlinePosts()).toEqual(['/api/projects/outline?dry_run=true']));
    await vi.waitFor(() => expect(document.body.textContent).toContain('2 items will be added'));

    const confirm = Array.from(document.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Create project')!;
    confirm.click();
    await vi.waitFor(() => expect(outlinePosts()).toEqual(['/api/projects/outline?dry_run=true', '/api/projects/outline']));
  });
});
