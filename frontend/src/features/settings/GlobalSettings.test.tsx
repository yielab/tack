import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import GlobalSettings from './GlobalSettings';

const flush = () => new Promise((r) => setTimeout(r, 0));
let fetchMock: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:x');
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = String(input);
    if (url.endsWith('/api/backup')) {
      return Promise.resolve(new Response(new Blob(['DB']), { status: 200 }));
    }
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={GlobalSettings} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

const findButton = (c: HTMLElement, text: string) =>
  Array.from(c.querySelectorAll('button')).find((b) => b.textContent?.includes(text))!;

describe('GlobalSettings', () => {
  it('downloads a backup from GET /backup', async () => {
    const { container } = mount();
    findButton(container, 'Download backup').click();
    await flush();
    expect(fetchMock.mock.calls.some((c) => String(c[0]).endsWith('/api/backup'))).toBe(true);
    expect(URL.createObjectURL).toHaveBeenCalled();
  });

  it('posts a restore only after the confirm dialog is accepted', async () => {
    const { container } = mount();
    const input = container.querySelector<HTMLInputElement>('input[type="file"]')!;
    const file = new File(['db'], 'backup.db');

    // Declined → no POST.
    vi.spyOn(window, 'confirm').mockReturnValueOnce(false);
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    input.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect(fetchMock.mock.calls.some((c) => String(c[0]).endsWith('/api/restore'))).toBe(false);

    // Accepted → POST /restore.
    vi.spyOn(window, 'confirm').mockReturnValueOnce(true);
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    input.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    const post = fetchMock.mock.calls.find(
      (c) => (c[1] as RequestInit)?.method === 'POST' && String(c[0]).endsWith('/api/restore'),
    );
    expect(post).toBeTruthy();
  });
});
