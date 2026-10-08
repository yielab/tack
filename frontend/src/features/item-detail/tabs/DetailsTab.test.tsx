import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import DetailsTab from './DetailsTab';
import type { Item } from '../../../shared/types';

const ITEM = { id: 'i1', project_id: 'p1', description: '' } as unknown as Item;

const flush = () => new Promise((r) => setTimeout(r, 0));
let fetchMock: ReturnType<typeof vi.spyOn>;
let linked = false;
let putResponse: (body: unknown) => Response;
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

beforeEach(() => {
  linked = false;
  putResponse = (body) => json(body);
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = (init as RequestInit)?.method ?? 'GET';
    if (url.endsWith('/api/items/i1/github-link') && method === 'GET') {
      return Promise.resolve(
        linked
          ? new Response(JSON.stringify({ repo: 'acme/widgets', issue_number: 42 }), { status: 200 })
          : new Response(null, { status: 404 }),
      );
    }
    if (url.endsWith('/api/items/i1/github-link') && method === 'PUT') {
      linked = true;
      return Promise.resolve(new Response(null, { status: 204 }));
    }
    if (url.endsWith('/api/items/i1/brief') && method === 'GET')
      return Promise.resolve(json({ error: { status: 404, message: 'Item i1 has no brief' } }, 404));
    if (url.endsWith('/api/items/i1/brief') && method === 'PUT')
      return Promise.resolve(putResponse(JSON.parse((init as RequestInit).body as string)));
    return Promise.resolve(new Response(JSON.stringify({}), { status: 200 }));
  });
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => <DetailsTab item={ITEM} onDescriptionChange={() => {}} />,
    container,
  );
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

describe('DetailsTab', () => {
  it('links a GitHub issue and then shows the linked state', async () => {
    const { container } = mount();
    await flush();

    const input = container.querySelector<HTMLInputElement>('input')!;
    input.value = 'acme/widgets#42';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    const linkButton = Array.from(container.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Link',
    )!;
    linkButton.click();
    await flush();
    await flush();

    const put = fetchMock.mock.calls.find(
      (c) =>
        (c[1] as RequestInit)?.method === 'PUT' &&
        String(c[0]).endsWith('/api/items/i1/github-link'),
    );
    expect(put).toBeTruthy();
    expect(JSON.parse((put![1] as RequestInit).body as string)).toEqual({
      repo: 'acme/widgets',
      issue_number: 42,
    });

    expect(container.textContent).toContain('acme/widgets#42');
  });

  it('acceptance_sentences_save_as_manual_criteria', async () => {
    const { container } = mount();
    await flush();
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

    const puts = fetchMock.mock.calls.filter(
      (c) => (c[1] as RequestInit)?.method === 'PUT' && String(c[0]).endsWith('/api/items/i1/brief'),
    );
    const body = JSON.parse((puts[puts.length - 1][1] as RequestInit).body as string);
    expect(body.acceptance).toEqual([
      { kind: 'manual', id: 'm1', title: one.slice(0, 60), text: one },
      { kind: 'manual', id: 'm2', title: two.slice(0, 60), text: two },
    ]);
  });

  it('keeps_the_agent_block_collapsed_with_a_count', async () => {
    fetchMock.mockImplementation((input, init) => {
      const url = String(input);
      if (url.endsWith('/api/items/i1/brief') && ((init as RequestInit)?.method ?? 'GET') === 'GET')
        return Promise.resolve(
          json({
            acceptance: [
              { kind: 'manual', id: 'a', title: 'a', text: 'a' },
              { kind: 'command', id: 'b', title: 'b', run: 'x', expect_exit: 0, cwd: null },
              { kind: 'test', id: 'c', title: 'c', name: 't', runner: null },
            ],
            constraints: [{ kind: 'note', text: 'n' }],
            definition_of_done: null,
            risk: null,
          }),
        );
      return Promise.resolve(json({}, 404));
    });
    const { container } = mount();
    await flush();
    const block = container.querySelector<HTMLDetailsElement>('details')!;
    expect(block.open).toBe(false);
    expect(block.querySelector('summary')!.textContent).toContain('For the agent');
    expect(block.querySelector('summary')!.textContent).toContain('2 checks · 1 limit');
  });

  it('save_sends_the_typed_brief', async () => {
    const { container } = mount();
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

    const puts = fetchMock.mock.calls.filter((c) => (c[1] as RequestInit)?.method === 'PUT' && String(c[0]).endsWith('/brief'));
    expect(JSON.parse((puts[puts.length - 1][1] as RequestInit).body as string)).toEqual({
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
    const { container } = mount();
    await flush();
    button(container, 'Add criterion').click();
    await flush();
    expect(container.querySelector<HTMLInputElement>('[aria-label="Expected exit code"]')!.value).toBe('0');
  });

  it('renders a server validation error beside the criteria', async () => {
    putResponse = () => json({ error: { status: 400, message: 'acceptance: criterion id `m1` is used twice' } }, 400);
    const { container } = mount();
    await flush();
    button(container, 'Add check').click();
    await flush();
    type(container, 'Acceptance criterion', 'Something');
    blur(container, 'Acceptance criterion');
    await flush();
    const alert = container.querySelector('[role="alert"]')!;
    expect(alert.textContent).toBe('criterion id `m1` is used twice');
    expect(alert.closest('section')!.textContent).toContain('Acceptance criteria');
  });
});
