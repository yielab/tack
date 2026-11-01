import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ProposeFromFile from './ProposeFromFile';
import { flush } from '../../test/flush';
const disposers: Array<() => void> = [];

const PROJECT = { id: 'p1', vocabulary: {} };

function routeFetch(created: unknown, posts: Array<{ url: string; body: any }>) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const json = (body: unknown, status = 200) =>
      Promise.resolve(new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } }));
    if (init?.method === 'POST') {
      posts.push({ url, body: JSON.parse(String(init.body)) });
      return json(created, 201);
    }
    if (url.includes('/agent-profiles')) return json({ protocol_version: 1, data: [] });
    if (url.includes('/teams')) return json({ teams: [] });
    if (url.includes('/roles')) return json([]);
    if (url.includes('/projects/')) return json(PROJECT);
    return json({ id: 'item_1', project_id: 'p1', roles: [] });
  });
}

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ProposeFromFile itemId="item_1" />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

async function settle() {
  await flush();
  await flush();
  await flush();
}

/** Picks a file in the input, the way the browser does when the person chooses one. */
async function pick(container: HTMLElement, text: string, name = 'tack-plan.json') {
  const input = container.querySelector<HTMLInputElement>('input[type="file"]')!;
  const file = new File([text], name, { type: 'application/json' });
  Object.defineProperty(input, 'files', { value: [file], configurable: true });
  input.dispatchEvent(new Event('change'));
  await settle();
}

const VALID = {
  v: '1',
  subtasks: [
    { title: 'First', description: 'Do it.', acceptance: ['It is done'], depends_on: [], estimate: 'S', type: 'task' },
    { title: 'Second', description: 'Then this.', acceptance: [], depends_on: [0], parent: 0, role: 'Backend' },
  ],
};

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('ProposeFromFile', () => {
  it('a_file_with_a_bad_entry_names_the_entry_and_why', async () => {
    routeFetch({ created: [] }, []);
    const container = mount();
    const bad = { v: '1', subtasks: [{ title: 'First', description: '' }, { title: 'Expose', description: '', depends_on: [6] }] };
    await pick(container, JSON.stringify(bad));

    const alert = container.querySelector('[role="alert"]');
    expect(alert?.textContent).toContain('Entry 2 ("Expose")');
    expect(alert?.textContent).toContain('"depends_on" names entry 7');
    expect(container.querySelector('[data-testid="plan-panel"]')).toBeNull();
  });

  it('a_file_that_is_not_json_says_so', async () => {
    routeFetch({ created: [] }, []);
    const container = mount();
    await pick(container, '{ not json');
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('The file is not JSON');
  });

  it('a_valid_file_opens_the_panel_and_creates_with_no_artifact_id', async () => {
    const posts: Array<{ url: string; body: any }> = [];
    routeFetch({ created: [{ id: 'n1', title: 'First' }, { id: 'n2', title: 'Second' }] }, posts);
    const container = mount();
    await pick(container, JSON.stringify(VALID));

    expect(container.querySelector('[data-testid="plan-panel"]')).not.toBeNull();
    expect(container.textContent).toContain('This file proposes 2 items');
    expect(posts.length).toBe(0);

    Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Create 2 items')!.click();
    await settle();

    expect(posts.length).toBe(1);
    expect(posts[0].url).toContain('/items/item_1/subtasks-from-plan');
    expect('artifact_id' in posts[0].body).toBe(false);
    expect(posts[0].body.subtasks[1]).toMatchObject({ title: 'Second', depends_on: [0], parent: 0, role: 'Backend' });
    expect(container.textContent).toContain('Created 2 items');
  });

  it('a_dropped_file_opens_the_same_panel', async () => {
    routeFetch({ created: [] }, []);
    const container = mount();
    await drop(container, JSON.stringify(VALID));

    expect(container.querySelector('[data-testid="plan-panel"]')).not.toBeNull();
    expect(container.textContent).toContain('This file proposes 2 items');
  });

  it('a_dropped_file_that_does_not_fit_names_the_entry_and_why', async () => {
    routeFetch({ created: [] }, []);
    const container = mount();
    const bad = { v: '1', subtasks: [{ title: 'First', description: '' }, { title: 'Expose', description: '', depends_on: [6] }] };
    await drop(container, JSON.stringify(bad));

    const alert = container.querySelector('[role="alert"]');
    expect(alert?.textContent).toContain('Entry 2 ("Expose")');
    expect(alert?.textContent).toContain('"depends_on" names entry 7');
    expect(container.querySelector('[data-testid="plan-panel"]')).toBeNull();
  });

  it('the_drop_zone_shows_a_dashed_border_while_a_file_is_over_it', async () => {
    routeFetch({ created: [] }, []);
    const container = mount();
    const zone = container.querySelector<HTMLElement>('input[type="file"]')!.parentElement!.parentElement!;
    const resting = zone.getAttribute('style');
    zone.dispatchEvent(new Event('dragover', { bubbles: true, cancelable: true }));
    await flush();
    expect(zone.getAttribute('style')).not.toBe(resting);
    expect(zone.getAttribute('style')).toContain('dashed');
  });
});

/** Drops a file on the drop zone, the way the browser does when a person drags one onto it. */
async function drop(container: HTMLElement, text: string, name = 'tack-plan.json') {
  const target = container.querySelector<HTMLInputElement>('input[type="file"]')!;
  const file = new File([text], name, { type: 'application/json' });
  const event = new Event('drop', { bubbles: true, cancelable: true });
  Object.defineProperty(event, 'dataTransfer', { value: { files: [file] } });
  target.dispatchEvent(event);
  await settle();
}
