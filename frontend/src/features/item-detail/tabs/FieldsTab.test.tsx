import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import FieldsTab from './FieldsTab';
import type { Item } from '../../../shared/types';

const ITEM = { id: 'i1', project_id: 'p1' } as unknown as Item;

const defs = [
  { id: 'f1', project_id: 'p1', name: 'Client', field_type: 'text', description: null, required: false, default_value: null, options: null, validation: null, created_at: '', updated_at: '' },
];
const roles = [{ id: 'r1', project_id: 'p1', name: 'Developer', color: '#fff', icon: null, created_at: '' }];

const flush = () => new Promise((r) => setTimeout(r, 0));
let fetchMock: ReturnType<typeof vi.spyOn>;

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

beforeEach(() => {
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = (init as RequestInit)?.method ?? 'GET';
    if (url.endsWith('/api/projects/p1/custom-fields') && method === 'GET')
      return Promise.resolve(new Response(JSON.stringify(defs), { status: 200 }));
    if (url.endsWith('/api/items/i1/custom-fields') && method === 'GET')
      return Promise.resolve(new Response(JSON.stringify([]), { status: 200 }));
    if (url.endsWith('/api/projects/p1/roles') && method === 'GET')
      return Promise.resolve(new Response(JSON.stringify(roles), { status: 200 }));
    return Promise.resolve(new Response(JSON.stringify({}), { status: 200 }));
  });
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <FieldsTab item={ITEM} />, container);
  return { container, dispose };
}

describe('FieldsTab', () => {
  it('renders one input per field definition', async () => {
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('Client');
    expect(container.querySelectorAll('input[type="text"]').length).toBe(1);
  });

  it('PUTs the value when a custom field changes', async () => {
    const { container } = mount();
    await flush();

    const input = container.querySelector<HTMLInputElement>('input[type="text"]')!;
    input.value = 'Acme';
    input.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();

    const put = fetchMock.mock.calls.find(
      (c) => (c[1] as RequestInit)?.method === 'PUT' && String(c[0]).endsWith('/api/items/i1/custom-fields/f1'),
    );
    expect(put).toBeTruthy();
    expect(JSON.parse((put![1] as RequestInit).body as string)).toBe('Acme');
  });

  it('assigns then unassigns a role with the right endpoints', async () => {
    const { container } = mount();
    await flush();

    const roleCheckbox = container.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    roleCheckbox.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect(
      fetchMock.mock.calls.some(
        (c) => (c[1] as RequestInit)?.method === 'PUT' && String(c[0]).endsWith('/api/items/i1/roles/r1'),
      ),
    ).toBe(true);

    roleCheckbox.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect(
      fetchMock.mock.calls.some(
        (c) => (c[1] as RequestInit)?.method === 'DELETE' && String(c[0]).endsWith('/api/items/i1/roles/r1'),
      ),
    ).toBe(true);
  });
});
