import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import FilesTab from './FilesTab';

const attachment = {
  id: 'a1',
  item_id: 'i1',
  filename: 'spec.pdf',
  mime_type: 'application/pdf',
  storage_path: '/x',
  size_bytes: 2048,
  uploaded_at: '',
};

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
    if (url.endsWith('/api/items/i1/attachments') && method === 'GET') {
      return Promise.resolve(new Response(JSON.stringify([attachment]), { status: 200 }));
    }
    return Promise.resolve(new Response(JSON.stringify(attachment), { status: 200 }));
  });
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <FilesTab itemId="i1" />, container);
  return { container, dispose };
}

function setFiles(input: HTMLInputElement, files: File[]) {
  Object.defineProperty(input, 'files', { value: files, configurable: true });
  input.dispatchEvent(new Event('change', { bubbles: true }));
}

describe('FilesTab', () => {
  it('lists attachment metadata with a download link', async () => {
    const { container } = mount();
    await flush();
    const link = container.querySelector<HTMLAnchorElement>('a[download]')!;
    expect(link.textContent).toBe('spec.pdf');
    expect(link.getAttribute('href')).toBe('/api/attachments/a1');
    expect(container.textContent).toContain('2.0 KB');
    expect(container.textContent).toContain('application/pdf');
  });

  it('uploads via multipart/form-data (no JSON content-type)', async () => {
    const { container } = mount();
    await flush();

    const input = container.querySelector<HTMLInputElement>('input[type="file"]')!;
    setFiles(input, [new File(['hello'], 'note.txt', { type: 'text/plain' })]);
    await flush();

    const post = fetchMock.mock.calls.find((c) => (c[1] as RequestInit)?.method === 'POST');
    expect(post).toBeTruthy();
    expect(String(post![0])).toBe('/api/items/i1/attachments');
    expect((post![1] as RequestInit).body).toBeInstanceOf(FormData);
    const headers = (post![1] as RequestInit).headers as Headers;
    expect(headers.has('Content-Type')).toBe(false);
  });

  it('rejects an oversize file before uploading', async () => {
    const { container } = mount();
    await flush();

    const big = new File(['x'], 'big.bin');
    Object.defineProperty(big, 'size', { value: 60 * 1024 * 1024 });
    const input = container.querySelector<HTMLInputElement>('input[type="file"]')!;
    setFiles(input, [big]);
    await flush();

    // no POST happened
    expect(fetchMock.mock.calls.some((c) => (c[1] as RequestInit)?.method === 'POST')).toBe(false);
  });
});
