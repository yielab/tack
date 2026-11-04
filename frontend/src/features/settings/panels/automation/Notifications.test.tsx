import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import Notifications, { channelLine } from './Notifications';
import { flush } from '../../../../test/flush';

const disposers: Array<() => void> = [];
const settle = async () => {
  for (let i = 0; i < 6; i += 1) await flush();
};

function mount(answer: Response) {
  vi.spyOn(globalThis, 'fetch').mockImplementation(async () => answer);
  const container = document.createElement('div');
  document.body.appendChild(container);
  disposers.push(render(() => <Notifications />, container));
  return container;
}

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

afterEach(() => {
  disposers.splice(0).forEach((dispose) => dispose());
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('Notification channels', () => {
  it('says_so_when_no_channel_is_set', async () => {
    const container = mount(json([]));
    await settle();
    expect(container.querySelectorAll('[data-testid="notification-channel"]')).toHaveLength(0);
    expect(container.textContent).toContain('No channel is set');
  });

  it('shows_one_line_per_channel_with_what_waits_and_what_failed', async () => {
    const container = mount(json([{ name: 'webhook', pending: 2, failed: 1, last_error: 'the webhook answered 404 Not Found' }]));
    await settle();
    const lines = Array.from(container.querySelectorAll('[data-testid="notification-channel"]')).map((e) => e.textContent);
    expect(lines).toEqual(['Webhook · 2 waiting, 1 given up. Last error: the webhook answered 404 Not Found']);
    expect(container.textContent).not.toContain('No channel is set');
  });

  it('leaves_the_error_out_when_there_is_none', () => {
    expect(channelLine({ name: 'webhook', pending: 0, failed: 0, last_error: null })).toBe('0 waiting, 0 given up');
  });

  it('is_told_of_a_failed_read', async () => {
    const container = mount(json({ error: 'The board is unreachable' }, 500));
    await settle();
    expect(container.textContent).toContain('the notification channels');
    expect(container.textContent).not.toContain('No channel is set');
  });
});
