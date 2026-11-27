import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import GithubIssueSection from './GithubIssueSection';
import type { Item } from '../../../shared/types';
import { flush } from '../../../test/flush';

const ITEM = { id: 'i1', project_id: 'p1' } as unknown as Item;
let fetchMock: ReturnType<typeof vi.spyOn>;
let linked = false;

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

beforeEach(() => {
  linked = false;
  localStorage.clear();
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
  const dispose = render(() => <GithubIssueSection item={ITEM} />, container);
  return { container, dispose };
}

describe('GithubIssueSection', () => {
  it('links a GitHub issue and then shows the linked state', async () => {
    const { container } = mount();
    await flush();
    container.querySelector<HTMLButtonElement>('[data-fold="github"] > button')!.click();

    const input = container.querySelector<HTMLInputElement>('input')!;
    input.value = 'acme/widgets#42';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();
    [...container.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Link')!.click();
    await flush();
    await flush();

    const put = fetchMock.mock.calls.find(
      (c) => (c[1] as RequestInit)?.method === 'PUT' && String(c[0]).endsWith('/api/items/i1/github-link'),
    );
    expect(JSON.parse((put![1] as RequestInit).body as string)).toEqual({ repo: 'acme/widgets', issue_number: 42 });
    expect(container.textContent).toContain('acme/widgets#42');
  });

  it('keeps the link folded, and says in its header whether an issue is linked', async () => {
    const { container } = mount();
    await flush();
    const head = container.querySelector<HTMLButtonElement>('[data-fold="github"] > button')!;
    expect(head.getAttribute('aria-expanded')).toBe('false');
    expect(head.textContent).toContain('GitHub issue');
    expect(head.textContent).toContain('no GitHub issue linked');
    head.click();
    expect(head.getAttribute('aria-expanded')).toBe('true');
  });
});
