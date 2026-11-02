import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { flush } from '../../test/flush';
import { ITEM_UPDATED_EVENT } from './itemEvents';
import { useAutoStart } from './autoStart';

const FULL = { code: 'max_open', text: '3 runs are already open, the most this project allows' };
let waiting: Array<{ item_id: string; why: typeof FULL }> = [];
const disposers: Array<() => void> = [];

function answer() {
  return vi.spyOn(globalThis, 'fetch').mockImplementation(
    (async () =>
      new Response(JSON.stringify({ enabled: true, status: 'To Do', waiting }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      })) as typeof fetch,
  );
}

function Reader(props: { project: string; item: string }) {
  const { waiting: reason } = useAutoStart(() => props.project);
  return <p data-item={props.item}>{reason(props.item)?.text ?? 'nothing'}</p>;
}

function mount(...readers: Array<[string, string]>) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => readers.map(([project, item]) => <Reader project={project} item={item} />), container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
  vi.useRealTimers();
  waiting = [];
});

describe('useAutoStart', () => {
  it('reads the reason of a waiting task and nothing for another, loading once per project however many ask', async () => {
    waiting = [{ item_id: 'a', why: FULL }];
    const fetching = answer();
    const page = mount(['p-shared', 'a'], ['p-shared', 'b']);
    await flush();
    expect([...page.querySelectorAll('p')].map((p) => p.textContent)).toEqual([FULL.text, 'nothing']);
    const autoStartCalls = fetching.mock.calls.filter(([url]) => String(url).endsWith('/projects/p-shared/auto-start'));
    expect(autoStartCalls).toHaveLength(1);
  });

  it('reads again when an item was edited, and shows what the server now says', async () => {
    waiting = [{ item_id: 'a', why: FULL }];
    answer();
    const page = mount(['p-edited', 'a']);
    await flush();
    expect(page.textContent).toBe(FULL.text);
    waiting = [];
    window.dispatchEvent(new Event(ITEM_UPDATED_EVENT));
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(page.textContent).toBe('nothing');
  });

  it('keeps what it showed when the answer cannot be read', async () => {
    waiting = [{ item_id: 'a', why: FULL }];
    const fetching = answer();
    const page = mount(['p-failing', 'a']);
    await flush();
    fetching.mockImplementation((async () => new Response('{}', { status: 500 })) as typeof fetch);
    window.dispatchEvent(new Event(ITEM_UPDATED_EVENT));
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(page.textContent).toBe(FULL.text);
  });
});
