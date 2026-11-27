import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ProjectFolderCard from './ProjectFolderCard';
import { statusLine } from './planText';
import { folderPaths, type FolderPlan } from '../../../../shared/api/folder';
import { writeFolder, type DirectoryHandle } from './folderAccess';
import { flush } from '../../../../test/flush';

afterEach(() => {
  vi.restoreAllMocks();
  delete (window as unknown as { showDirectoryPicker?: unknown }).showDirectoryPicker;
  document.body.innerHTML = '';
});

const REVISION = { number: 7, origin: 'board', hash: 'abc', created_at: '2026-11-26T10:00:00Z' };

const plan = (over: Partial<FolderPlan> = {}): FolderPlan => ({
  plan_hash: 'h1',
  base: REVISION as FolderPlan['base'],
  changes: [
    { section: 'roles', key: 'contenido', name: 'Content', verb: 'update', fields: [{ path: 'name', from: 'Contenido', to: 'Content' }] },
    { section: 'items', key: 'login', name: 'Login with GitHub', verb: 'keep', fields: [], reason: "Kept: 'Login with GitHub' has 2 runs and 3 comments; close it on the board." },
    { section: 'sprints', key: 'semana-42', name: 'Semana 42', verb: 'remove', fields: [] },
  ],
  conflicts: [],
  removals: [{ section: 'sprints', key: 'semana-42', name: 'Semana 42', cost: ['3 cards leave it'] }],
  warnings: [],
  applies: true,
  ...over,
});

interface Call { method: string; url: string; body?: unknown }

function serve(answer: (call: Call) => Response | undefined) {
  const calls: Call[] = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
    const url = String(input);
    const call: Call = { method: init?.method ?? 'GET', url, body: init?.body ? JSON.parse(String(init.body)) : undefined };
    calls.push(call);
    const answered = answer(call);
    if (answered) return answered;
    if (url.endsWith('/folder/status')) return json({ last_revision: REVISION, board_hash: 'b', changes: [] });
    if (url.endsWith('/folder/revisions')) return json([{ ...REVISION, number: 8, origin: 'file' }, REVISION]);
    if (url.includes('/folder/plan')) return json(plan());
    return json({});
  });
  return calls;
}

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

async function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <ProjectFolderCard projectId="p1" />, container);
  await flush();
  return container;
}

function file(path: string, text: string): File {
  const f = new File([text], path.split('/').pop()!);
  Object.defineProperty(f, 'webkitRelativePath', { value: path });
  return f;
}

async function pickFolder(container: HTMLElement, files: File[]) {
  const input = container.querySelector<HTMLInputElement>('[data-testid="project-folder-input"]')!;
  Object.defineProperty(input, 'files', { value: files, configurable: true });
  input.dispatchEvent(new Event('change', { bubbles: true }));
  await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).not.toBeNull());
  await flush();
}

const button = (label: string) =>
  Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === label);

describe('statusLine', () => {
  it('counts what the board did since the last revision, by section', () => {
    const changes = [
      { section: 'items', key: 'a', name: 'A', verb: 'create' as const, fields: [] },
      { section: 'items', key: 'b', name: 'B', verb: 'create' as const, fields: [] },
      { section: 'roles', key: 'dev', name: 'Dev', verb: 'update' as const, fields: [] },
    ];
    expect(statusLine({ last_revision: REVISION as never, board_hash: '', changes })).toBe(
      '2 items created and 1 role changed on the board since revision #7.',
    );
    expect(statusLine({ last_revision: REVISION as never, board_hash: '', changes: [] })).toBe('The board matches revision #7.');
    expect(statusLine({ last_revision: null, board_hash: '', changes: [] })).toContain('until then an import removes nothing');
  });
});

describe('folderPaths', () => {
  it('reads a picked .tack folder by its own paths, and a picked repository only under .tack', () => {
    expect([...folderPaths(['.tack/project.yaml', '.tack/items/login.md']).values()]).toEqual(['project.yaml', 'items/login.md']);
    expect([...folderPaths(['repo/src/main.rs', 'repo/.tack/project.yaml', 'repo/.tack/items/a.md']).entries()]).toEqual([
      ['repo/.tack/project.yaml', 'project.yaml'],
      ['repo/.tack/items/a.md', 'items/a.md'],
    ]);
    expect([...folderPaths(['plan/project.yaml']).values()]).toEqual(['project.yaml']);
  });
});

describe('ProjectFolderCard', () => {
  it('says where the folder stands against the board', async () => {
    serve(() => undefined);
    const container = await mount();
    expect(container.querySelector('[data-testid="project-folder-status"]')!.textContent).toBe('The board matches revision #7.');
  });

  it('plans a picked folder, lists every change by section, and lets Apply go only once the removals are read', async () => {
    const calls = serve(() => undefined);
    const container = await mount();
    await pickFolder(container, [file('repo/.tack/project.yaml', 'name: My SaaS'), file('repo/.tack/items/login.md', '---\ntitle: Login\n---\n'), file('repo/README.md', '# x')]);

    const planned = calls.find((c) => c.url.endsWith('/folder/plan'))!;
    expect(planned.method).toBe('POST');
    expect(planned.body).toEqual({ files: { 'project.yaml': 'name: My SaaS', 'items/login.md': '---\ntitle: Login\n---\n' } });

    const sections = Array.from(document.querySelectorAll('[data-testid="plan-section"]')).map((s) => s.getAttribute('aria-label'));
    expect(sections).toEqual(['Roles', 'Items']);
    const text = document.querySelector('[role="dialog"]')!.textContent!;
    expect(text).toContain('name: “Contenido” → “Content”');
    expect(text).toContain('has 2 runs and 3 comments');
    // The removal is listed once, under Removed, with its cost.
    expect(document.querySelectorAll('[data-testid="plan-change"][data-verb="remove"]')).toHaveLength(0);
    expect(document.querySelector('[data-testid="plan-removals"]')!.textContent).toContain('3 cards leave it');

    expect(button('Apply')!.disabled).toBe(true);
    const box = document.querySelector<HTMLInputElement>('[data-testid="plan-removals"] input[type="checkbox"]')!;
    box.click();
    await flush();
    expect(button('Apply')!.disabled).toBe(false);

    button('Apply')!.click();
    await flush();
    const applied = calls.find((c) => c.url.includes('/folder/apply'))!;
    expect(applied.url).toContain('plan_hash=h1');
    expect(applied.body).toEqual(planned.body);
  });

  it('turns a board changed since the preview into Preview again', async () => {
    serve((c) => (c.url.includes('/folder/apply') ? json({ error: 'The project changed since the preview; preview again' }, 409) : undefined));
    const container = await mount();
    await pickFolder(container, [file('.tack/project.yaml', 'name: x')]);
    document.querySelector<HTMLInputElement>('[data-testid="plan-removals"] input[type="checkbox"]')!.click();
    await flush();
    button('Apply')!.click();
    await vi.waitFor(() => expect(button('Preview again')).toBeDefined());
    expect(document.body.textContent).toContain('The project changed since the preview; preview again');
    expect(button('Apply')).toBeUndefined();
  });

  it("shows the board's refusal of an agent's apply", async () => {
    serve((c) => (c.url.includes('/folder/apply') ? json({ error: 'An agent may preview the project folder; a person applies it' }, 403) : c.url.includes('/folder/plan') ? json(plan({ removals: [], changes: plan().changes.slice(0, 1) })) : undefined));
    const container = await mount();
    await pickFolder(container, [file('.tack/project.yaml', 'name: x')]);
    button('Apply')!.click();
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"] [role="alert"]')?.textContent).toBe('An agent may preview the project folder; a person applies it'));
  });

  it('plans again with the side chosen for the conflicts, and shows choosing per conflict disabled', async () => {
    const conflict = { section: 'roles', key: 'dev', name: 'Dev', path: 'color', base: '#111', board: '#222', file: '#333' };
    const calls = serve((c) => (c.url.includes('/folder/plan') ? json(plan({ conflicts: [conflict], removals: [], applies: !c.url.includes('prefer=') ? false : true })) : undefined));
    const container = await mount();
    await pickFolder(container, [file('.tack/project.yaml', 'name: x')]);

    const conflicts = document.querySelector('[data-testid="plan-conflicts"]')!;
    expect(conflicts.textContent).toContain('Last revision: “#111”');
    expect(conflicts.textContent).toContain("The board's: “#222”");
    expect(conflicts.textContent).toContain("The folder's: “#333”");
    expect(button('Apply')!.disabled).toBe(true);
    const perConflict = Array.from(conflicts.querySelectorAll('label')).find((l) => l.textContent?.includes('Choose per conflict'))!;
    expect(perConflict.querySelector('input')!.disabled).toBe(true);

    const preferFolder = Array.from(conflicts.querySelectorAll('label')).find((l) => l.textContent === 'Prefer the folder')!;
    preferFolder.querySelector('input')!.click();
    await vi.waitFor(() => expect(calls.some((c) => c.url.includes('/folder/plan?prefer=file'))).toBe(true));
    await flush();
    expect(button('Apply')!.disabled).toBe(false);
  });

  it('goes back to a revision through the same plan', async () => {
    const calls = serve(() => undefined);
    const container = await mount();
    button('History')!.click();
    await vi.waitFor(() => expect(container.querySelectorAll('[data-testid="project-folder-revision"]')).toHaveLength(2));
    expect(container.querySelector('[data-testid="project-folder-revision"]')!.textContent).toContain('The latest');
    button('Revert to this…')!.click();
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')?.textContent).toContain('Go back to revision #7?'));
    const planned = calls.find((c) => c.url.includes('/folder/plan'))!;
    expect(planned.url).toContain('/folder/plan?revision=7');
    expect(planned.body).toBeUndefined();
  });

  it('shows Export disabled with its reason where the browser cannot write a folder', async () => {
    serve(() => undefined);
    const container = await mount();
    expect(button('Export')!.disabled).toBe(true);
    expect(container.querySelector('[data-testid="project-folder-no-write"]')!.textContent).toContain('tack folder export');
  });
});

describe('writeFolder', () => {
  it('writes into .tack/ inside the picked folder and removes the item files the board no longer has', async () => {
    const tree: Record<string, string> = { 'items/old.md': 'x', 'items/keep.md': 'y' };
    const dir = (prefix: string, name: string): DirectoryHandle => ({
      name,
      getDirectoryHandle: async (n) => dir(prefix === '' && name !== '.tack' ? '' : `${prefix}${n}/`, n),
      getFileHandle: async (n) => ({
        createWritable: async () => ({ write: async (t: string) => { tree[`${prefix}${n}`] = t; }, close: async () => {} }),
      }),
      removeEntry: async (n) => { delete tree[`${prefix}${n}`]; },
      keys: async function* () {
        for (const path of Object.keys(tree)) if (path.startsWith(prefix) && !path.slice(prefix.length).includes('/')) yield path.slice(prefix.length);
      },
    });
    const result = await writeFolder(dir('', 'repo'), { 'project.yaml': 'name: x', 'items/keep.md': 'z' });
    expect(result).toEqual({ written: 2, removed: 1 });
    expect(tree).toEqual({ 'project.yaml': 'name: x', 'items/keep.md': 'z' });
  });
});
