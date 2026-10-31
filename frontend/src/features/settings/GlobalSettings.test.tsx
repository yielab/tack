import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import GlobalSettings from './GlobalSettings';
import { flush } from '../../test/flush';
let fetchMock: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = String(input);
    if (url.endsWith('/api/backup')) {
      return Promise.resolve(new Response('DB', { status: 200 }));
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

function confirmDialog(text: string) {
  const btn = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find(
    (b) => b.textContent?.trim() === text,
  )!;
  btn.click();
}

const findButton = (c: HTMLElement, text: string) =>
  Array.from(c.querySelectorAll('button')).find((b) => b.textContent?.includes(text))!;

describe('GlobalSettings', () => {
  it('shows every cloud backup control disabled with its reason while cloud backup is not set up', async () => {
    const { container } = mount();
    await flush();
    for (const name of ['Back up now', 'Restore latest', 'Verify latest']) {
      const button = findButton(container, name);
      expect(button.disabled).toBe(true);
      expect(container.querySelector(`#${CSS.escape(button.getAttribute('aria-describedby')!)}`)!.textContent).toBe('Set up cloud backup first');
    }
    const refresh = container.querySelector<HTMLButtonElement>('button[aria-label="Refresh the list of cloud backups"]')!;
    expect(refresh.disabled).toBe(true);
  });

  it('downloads a backup from GET /backup', async () => {
    const { container } = mount();
    findButton(container, 'Download backup').click();
    await vi.waitFor(() => {
      expect(fetchMock.mock.calls.some((c) => String(c[0]).endsWith('/api/backup'))).toBe(true);
      expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
    });
    const downloaded = vi.mocked(URL.createObjectURL).mock.calls[0][0];
    expect(Object.prototype.toString.call(downloaded)).toBe('[object Blob]');
    expect(downloaded.size).toBe(2);
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:vitest/2');
  });

  it('posts a restore only after the confirm dialog is accepted', async () => {
    const { container } = mount();
    const input = container.querySelector<HTMLInputElement>('input[type="file"]')!;
    const file = new File(['db'], 'backup.db');

    // Declined → no POST.
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    input.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    confirmDialog('Cancel');
    await flush();
    expect(fetchMock.mock.calls.some((c) => String(c[0]).endsWith('/api/restore'))).toBe(false);

    // Accepted → POST /restore.
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    input.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    confirmDialog('Restore');
    await flush();
    const post = fetchMock.mock.calls.find(
      (c) => (c[1] as RequestInit)?.method === 'POST' && String(c[0]).endsWith('/api/restore'),
    );
    expect(post).toBeTruthy();
  });
});
