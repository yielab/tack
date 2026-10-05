import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import BriefTab from './BriefTab';

const flush = () => new Promise((r) => setTimeout(r, 0));
let fetchMock: ReturnType<typeof vi.spyOn>;
let putResponse: () => Response;

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

beforeEach(() => {
  putResponse = () => json({}, 200);
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = (init as RequestInit)?.method ?? 'GET';
    if (url.endsWith('/api/items/i1/brief') && method === 'GET')
      return Promise.resolve(json({ error: { status: 404, message: 'Item i1 has no brief' } }, 404));
    if (url.endsWith('/api/items/i1/brief') && method === 'PUT') return Promise.resolve(putResponse());
    return Promise.resolve(json({}));
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <BriefTab itemId="i1" />, container);
  return { container, dispose };
}

const button = (c: HTMLElement, name: string) =>
  [...c.querySelectorAll('button')].find((b) => b.textContent?.trim() === name)!;

function type(c: HTMLElement, label: string, value: string) {
  const el = c.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[aria-label="${label}"]`)!;
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

function pick(c: HTMLElement, label: string, value: string) {
  const el = c.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!;
  el.value = value;
  el.dispatchEvent(new Event('change', { bubbles: true }));
}

describe('BriefTab', () => {
  it('shows the expected exit code for a command criterion', async () => {
    const { container } = mount();
    await flush();
    button(container, 'Add criterion').click();
    await flush();
    expect(container.querySelector<HTMLInputElement>('[aria-label="Expected exit code"]')!.value).toBe('0');
  });

  it('save_sends_the_typed_brief', async () => {
    const { container } = mount();
    await flush();
    button(container, 'Add criterion').click();
    await flush();
    type(container, 'Title', 'Build passes');
    type(container, 'Command', 'cargo build');
    type(container, 'Expected exit code', '2');
    button(container, 'Add criterion').click();
    await flush();
    const second = container.querySelectorAll('[data-testid="criterion"]')[1] as HTMLElement;
    pick(second, 'Kind', 'manual');
    await flush();
    type(second, 'Title', 'Looks right');
    type(second, 'What a person must check', 'The page reads well');
    button(container, 'Add constraint').click();
    await flush();
    type(container, 'Glob, e.g. src/legacy/**', 'src/legacy/**');
    type(container, 'Definition of done', 'Shipped and quiet');
    pick(container, 'Risk', 'high');
    button(container, 'Save brief').click();
    await flush();

    const put = fetchMock.mock.calls.find((c) => (c[1] as RequestInit)?.method === 'PUT')!;
    expect(JSON.parse((put[1] as RequestInit).body as string)).toEqual({
      acceptance: [
        { kind: 'command', id: 'c1', title: 'Build passes', run: 'cargo build', expect_exit: 2, cwd: null },
        { kind: 'manual', id: 'c2', title: 'Looks right', text: 'The page reads well' },
      ],
      constraints: [{ kind: 'forbidden_path', glob: 'src/legacy/**' }],
      definition_of_done: 'Shipped and quiet',
      risk: 'high',
    });
  });

  it('renders a server validation error beside the criteria', async () => {
    putResponse = () =>
      json({ error: { status: 400, message: 'acceptance: criterion id `c1` is used twice' } }, 400);
    const { container } = mount();
    await flush();
    button(container, 'Add criterion').click();
    await flush();
    button(container, 'Save brief').click();
    await flush();
    const alert = container.querySelector('[role="alert"]')!;
    expect(alert.textContent).toBe('criterion id `c1` is used twice');
    expect(alert.closest('section')!.textContent).toContain('Acceptance criteria');
  });
});
