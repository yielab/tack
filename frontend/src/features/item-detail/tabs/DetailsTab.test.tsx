import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import DetailsTab from './DetailsTab';
import type { Item } from '../../../shared/types';

const ITEM = { id: 'i1', project_id: 'p1', description: '' } as unknown as Item;

const flush = () => new Promise((r) => setTimeout(r, 0));
let fetchMock: ReturnType<typeof vi.spyOn>;
let linked = false;

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

beforeEach(() => {
  linked = false;
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
});
