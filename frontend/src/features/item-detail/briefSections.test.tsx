import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import DescriptionSection from './DescriptionSection';
import DoneWhenSection from './DoneWhenSection';
import ForTheAgentSection from './ForTheAgentSection';
import { useBrief } from './useBrief';
import type { FolderMarks } from './useFolderMarks';
import type { Item } from '../../shared/types';
import { flush } from '../../test/flush';

const ITEM = { id: 'i1', project_id: 'p1', description: '' } as unknown as Item;
const noFolder: FolderMarks = { revision: () => null, say: () => null };
let fetchMock: ReturnType<typeof vi.spyOn>;
let putResponse: (body: unknown) => Response;
/** What `GET /items/i1/brief` answers; null is an item with no brief. */
let storedBrief: unknown = null;
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

beforeEach(() => {
  putResponse = (body) => json(body);
  localStorage.clear();
  storedBrief = null;
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = (init as RequestInit)?.method ?? 'GET';
    if (url.endsWith('/api/items/i1/brief') && method === 'GET')
      return Promise.resolve(storedBrief ? json(storedBrief) : json({ error: { status: 404, message: 'Item i1 has no brief' } }, 404));
    if (url.endsWith('/api/items/i1/brief') && method === 'PUT')
      return Promise.resolve(putResponse(JSON.parse((init as RequestInit).body as string)));
    if (url.includes('/agent-context'))
      return Promise.resolve(json({ text: 'You are asked to fix notes.py', limit: { sentence: 'Stops at 300 000 tokens', enforced_by: 'docket', source: 'profile', tokens: 300000 }, limits: {} }));
    return Promise.resolve(json({}));
  });
});

function Screen(props: { layout: 'page' | 'drawer'; item?: Item; marks?: FolderMarks }) {
  const brief = useBrief(() => 'i1');
  const marks = props.marks ?? noFolder;
  return (
    <>
      <DescriptionSection item={props.item ?? ITEM} onDescriptionChange={() => {}} marks={marks} />
      <DoneWhenSection brief={brief} marks={marks} />
      <ForTheAgentSection itemId="i1" brief={brief} marks={marks} layout={props.layout} />
    </>
  );
}

async function mount(layout: 'page' | 'drawer' = 'page', item?: Item, marks?: FolderMarks) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <Screen layout={layout} item={item} marks={marks} />, container);
  await flush();
  await flush();
  return { container, dispose };
}

const button = (c: HTMLElement, name: string) =>
  [...c.querySelectorAll('button')].find((b) => b.textContent?.trim() === name)!;

function type(c: HTMLElement, label: string, value: string) {
  const el = c.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[aria-label="${label}"]`)!;
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

function blur(c: HTMLElement, label: string) {
  c.querySelector<HTMLElement>(`[aria-label="${label}"]`)!.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
}

function pick(c: HTMLElement, label: string, value: string) {
  const el = c.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!;
  el.value = value;
  el.dispatchEvent(new Event('change', { bubbles: true }));
}

const lastPut = () => {
  const puts = fetchMock.mock.calls.filter((c) => (c[1] as RequestInit)?.method === 'PUT' && String(c[0]).endsWith('/brief'));
  return JSON.parse((puts[puts.length - 1][1] as RequestInit).body as string);
};

describe('Description', () => {
  it('renders a Markdown description instead of showing its markers', async () => {
    const { container, dispose } = await mount('page', { ...ITEM, description: '**Fix** the parser\n\n- one\n- two' } as Item);
    const shown = container.querySelector('[data-testid="description"]')!;
    expect(shown.querySelector('strong')?.textContent).toBe('Fix');
    expect(shown.querySelectorAll('li')).toHaveLength(2);
    expect(shown.textContent).not.toContain('**');
    dispose();
  });

  it('says it is in the project folder, in a project that has one', async () => {
    const { container, dispose } = await mount('page', undefined, { revision: () => 3, say: () => ({ kind: 'held' }) });
    expect(container.querySelector('#item-description')!.closest('section')!.querySelector('[data-testid="folder-mark"]')!.textContent).toBe('In the project folder');
    dispose();
  });
});

describe('Done when', () => {
  it('acceptance_sentences_save_as_manual_criteria', async () => {
    const { container } = await mount();
    const one = 'The login page shows an error when the password is wrong.';
    const two = 'Nothing else on the page moves when that error appears, on any screen size.';
    button(container, 'Add check').click();
    await flush();
    type(container, 'Acceptance criterion', one);
    blur(container, 'Acceptance criterion');
    await flush();
    button(container, 'Add check').click();
    await flush();
    const inputs = container.querySelectorAll<HTMLInputElement>('[aria-label="Acceptance criterion"]');
    inputs[1].value = two;
    inputs[1].dispatchEvent(new Event('input', { bubbles: true }));
    inputs[1].dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
    await flush();
    expect(lastPut().acceptance).toEqual([
      { kind: 'manual', id: 'm1', title: one.slice(0, 60), text: one },
      { kind: 'manual', id: 'm2', title: two.slice(0, 60), text: two },
    ]);
  });

  it('ticking_a_done_when_line_saves_checked_and_moves_the_progress', async () => {
    storedBrief = {
      acceptance: [
        { kind: 'manual', id: 'a', title: 'Copy reads well', text: 'Copy reads well', checked: true },
        { kind: 'manual', id: 'b', title: 'Works on a phone', text: 'Works on a phone' },
        { kind: 'manual', id: 'c', title: 'Docs updated', text: 'Docs updated' },
      ],
      constraints: [],
      definition_of_done: null,
      risk: null,
    };
    const { container } = await mount();
    expect(container.textContent).toContain('1 of 3');
    expect(container.querySelector('[role="img"]')!.getAttribute('aria-label')).toBe('1 of 3 checks ticked');

    container.querySelector<HTMLInputElement>('input[aria-label="Done: Works on a phone"]')!.click();
    await flush();

    expect(lastPut().acceptance).toEqual([
      { kind: 'manual', id: 'a', title: 'Copy reads well', text: 'Copy reads well', checked: true },
      { kind: 'manual', id: 'b', title: 'Works on a phone', text: 'Works on a phone', checked: true },
      { kind: 'manual', id: 'c', title: 'Docs updated', text: 'Docs updated' },
    ]);
    expect(container.textContent).toContain('2 of 3');
  });

  it('puts Up, Down and Remove in each line\'s ⋯ menu, and moving or removing saves', async () => {
    storedBrief = {
      acceptance: [
        { kind: 'manual', id: 'a', title: 'First', text: 'First' },
        { kind: 'manual', id: 'b', title: 'Second', text: 'Second' },
      ],
      constraints: [],
      definition_of_done: null,
      risk: null,
    };
    const { container } = await mount();
    expect(button(container, 'Up')).toBeUndefined();
    const first = container.querySelector<HTMLButtonElement>('button[aria-label="Actions for check 1"]')!;
    first.click();
    await flush();
    const entries = [...container.querySelectorAll('[role="menuitem"]')].map((e) => e.textContent);
    expect(entries).toEqual(['Move down', 'Remove']);
    [...container.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((e) => e.textContent === 'Move down')!.click();
    await flush();
    expect(lastPut().acceptance.map((c: { id: string }) => c.id)).toEqual(['b', 'a']);
    container.querySelector<HTMLButtonElement>('button[aria-label="Actions for check 2"]')!.click();
    await flush();
    [...container.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((e) => e.textContent === 'Remove')!.click();
    await flush();
    expect(lastPut().acceptance.map((c: { id: string }) => c.id)).toEqual(['b']);
  });

  it('renders a server validation error beside the criteria', async () => {
    putResponse = () => json({ error: { status: 400, message: 'acceptance: criterion id `m1` is used twice' } }, 400);
    const { container } = await mount();
    button(container, 'Add check').click();
    await flush();
    type(container, 'Acceptance criterion', 'Something');
    blur(container, 'Acceptance criterion');
    await flush();
    const alert = container.querySelector('[role="alert"]')!;
    expect(alert.textContent).toBe('criterion id `m1` is used twice');
    expect(alert.closest('section')!.textContent).toContain('Done when');
  });
});

describe('For the agent', () => {
  it('reads as a table of checks, the limits, the risk and the run\'s token limit, with a preview of everything the agent reads', async () => {
    storedBrief = {
      acceptance: [{ kind: 'command', id: 'b', title: 'The tests pass', run: 'python3 -m pytest -q', expect_exit: 0, cwd: null }],
      constraints: [{ kind: 'forbidden_path', glob: 'src/legacy/**' }],
      definition_of_done: null,
      risk: 'low',
    };
    const { container } = await mount();
    const row = container.querySelector('[data-testid="criterion-row"]')!;
    expect(row.textContent).toContain('The tests pass');
    expect(row.textContent).toContain('python3 -m pytest -q · exit 0');
    expect(container.textContent).toContain('Keeps out of src/legacy/**');
    expect(container.querySelector('[data-testid="agent-risk"]')!.textContent).toBe('Low');
    expect(container.querySelector('[data-testid="agent-token-limit"]')!.textContent).toBe('Stops at 300 000 tokens');
    expect(container.querySelector('[data-testid="agent-preview"]')).toBeNull();
    button(container, 'Preview everything the agent will read →').click();
    await flush();
    expect(container.querySelector('[data-testid="agent-preview"]')!.textContent).toBe('You are asked to fix notes.py');
  });

  it('keeps_the_agent_block_collapsed_with_a_count in the drawer', async () => {
    storedBrief = {
      acceptance: [
        { kind: 'manual', id: 'a', title: 'a', text: 'a' },
        { kind: 'command', id: 'b', title: 'b', run: 'x', expect_exit: 0, cwd: null },
        { kind: 'test', id: 'c', title: 'c', name: 't', runner: null },
      ],
      constraints: [{ kind: 'note', text: 'n' }],
      definition_of_done: null,
      risk: 'medium',
    };
    const { container } = await mount('drawer');
    const head = container.querySelector<HTMLButtonElement>('[data-fold="for-the-agent"] > button')!;
    expect(head.getAttribute('aria-expanded')).toBe('false');
    expect(head.textContent).toContain('For the agent');
    expect(head.textContent).toContain('2 checks · 1 limit · risk medium');
  });

  it('save_sends_the_typed_brief', async () => {
    const { container } = await mount();
    button(container, 'Edit').click();
    await flush();
    button(container, 'Add criterion').click();
    await flush();
    type(container, 'Title', 'Build passes');
    type(container, 'Command', 'cargo build');
    type(container, 'Expected exit code', '2');
    button(container, 'Add check').click();
    await flush();
    type(container, 'Acceptance criterion', 'The page reads well');
    button(container, 'Add constraint').click();
    await flush();
    type(container, 'Glob, e.g. src/legacy/**', 'src/legacy/**');
    pick(container, 'Risk', 'high');
    button(container, 'Save for the agent').click();
    await flush();
    expect(lastPut()).toEqual({
      acceptance: [
        { kind: 'manual', id: 'm2', title: 'The page reads well', text: 'The page reads well' },
        { kind: 'command', id: 'm1', title: 'Build passes', run: 'cargo build', expect_exit: 2, cwd: null },
      ],
      constraints: [{ kind: 'forbidden_path', glob: 'src/legacy/**' }],
      definition_of_done: null,
      risk: 'high',
    });
  });

  it('shows the expected exit code for a command criterion', async () => {
    const { container } = await mount();
    button(container, 'Edit').click();
    await flush();
    button(container, 'Add criterion').click();
    await flush();
    expect(container.querySelector<HTMLInputElement>('[aria-label="Expected exit code"]')!.value).toBe('0');
  });
});
