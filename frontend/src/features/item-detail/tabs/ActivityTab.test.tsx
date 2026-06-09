import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ActivityTab from './ActivityTab';

const flush = () => new Promise((r) => setTimeout(r, 0));
let fetchMock: ReturnType<typeof vi.spyOn>;

const existing = [
  {
    id: 'c1',
    item_id: 'i1',
    author: 'Ada',
    content: 'first comment',
    comment_type: 'comment',
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
];

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ActivityTab itemId="i1" />, container);
  return { container, dispose };
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('ActivityTab', () => {
  beforeEach(() => {
    fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit)?.method ?? 'GET';
      if (url.endsWith('/api/items/i1/comments') && method === 'GET') {
        return Promise.resolve(new Response(JSON.stringify(existing), { status: 200 }));
      }
      // default POST handled per-test via mockImplementationOnce
      return Promise.resolve(new Response(JSON.stringify({}), { status: 200 }));
    });
  });

  it('renders fetched comments', async () => {
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('first comment');
    expect(container.textContent).toContain('Ada');
  });

  it('optimistically appends a posted comment', async () => {
    const { container } = mount();
    await flush();

    // server returns the created comment
    fetchMock.mockImplementationOnce(() =>
      Promise.resolve(
        new Response(
          JSON.stringify({
            id: 'c2',
            item_id: 'i1',
            author: null,
            content: 'hello world',
            comment_type: 'comment',
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          }),
          { status: 200 },
        ),
      ),
    );

    const textarea = container.querySelector('textarea')!;
    textarea.value = 'hello world';
    textarea.dispatchEvent(new Event('input', { bubbles: true }));
    container.querySelector('form')!.dispatchEvent(new Event('submit'));

    // appears immediately (optimistic)
    expect(container.textContent).toContain('hello world');
    await flush();
    // still present after the server responds
    expect(container.textContent).toContain('hello world');
  });

  it('rolls back and keeps the draft when the post fails', async () => {
    const { container } = mount();
    await flush();

    fetchMock.mockImplementationOnce(() =>
      Promise.resolve(new Response('nope', { status: 500 })),
    );

    const textarea = container.querySelector('textarea')!;
    textarea.value = 'will fail';
    textarea.dispatchEvent(new Event('input', { bubbles: true }));
    container.querySelector('form')!.dispatchEvent(new Event('submit'));
    expect(container.textContent).toContain('will fail'); // optimistic

    await flush();
    // rolled back from the list…
    expect(container.querySelectorAll('li').length).toBe(1);
    // …and restored to the composer for retry
    expect(container.querySelector('textarea')!.value).toBe('will fail');
  });
});
