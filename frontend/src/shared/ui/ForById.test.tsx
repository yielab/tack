import { describe, it, expect, afterEach } from 'vitest';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { ForById } from './ForById';

afterEach(() => { document.body.innerHTML = ''; });

type Row = { id: string; title: string };

describe('ForById', () => {
  it('keeps a row mounted when its object is replaced, and shows the new content', () => {
    const [rows, setRows] = createSignal<Row[]>([{ id: 'a', title: 'one' }, { id: 'b', title: 'two' }]);
    let mounts = 0;
    const container = document.createElement('div');
    document.body.appendChild(container);
    render(() => (
      <ForById each={rows()}>
        {(row) => { mounts++; return <input data-id={row().id} value={row().title} />; }}
      </ForById>
    ), container);
    expect(mounts).toBe(2);
    const first = container.querySelector('[data-id="a"]');

    // What a refetch does: every object new, one of them changed.
    setRows([{ id: 'a', title: 'one, edited' }, { id: 'b', title: 'two' }]);
    expect(mounts).toBe(2);
    expect(container.querySelector('[data-id="a"]')).toBe(first);
    expect((first as HTMLInputElement).value).toBe('one, edited');
  });

  it('mounts new ids and drops removed ones', () => {
    const [rows, setRows] = createSignal<Row[]>([{ id: 'a', title: 'one' }]);
    const container = document.createElement('div');
    document.body.appendChild(container);
    render(() => (
      <ForById each={rows()}>{(row) => <span data-id={row().id}>{row().title}</span>}</ForById>
    ), container);
    setRows([{ id: 'b', title: 'two' }]);
    expect(container.querySelector('[data-id="a"]')).toBeNull();
    expect(container.querySelector('[data-id="b"]')?.textContent).toBe('two');
  });
});
