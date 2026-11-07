import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ReviewActions from './ReviewActions';
import { flush } from '../../test/flush';
const disposers: Array<() => void> = [];
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

describe('ReviewActions', () => {
  it('Reject… asks for the reason before it sends the verdict', async () => {
    const calls: Array<[string, RequestInit | undefined]> = [];
    vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
      calls.push([String(input), init]);
      const review = { verdict: 'rejected', reason: 'too broad', reviewed_by: 'operator:local', reviewed_at: '2026-10-19T10:00:00Z' };
      return new Response(JSON.stringify({ protocol_version: 1, data: [{ attempt_number: 1, review }] }), { status: 200 });
    }) as typeof fetch);
    const onReviewed = vi.fn();
    const c = document.createElement('div');
    document.body.appendChild(c);
    disposers.push(render(() => <ReviewActions requestId="req-1" attemptNumber={1} review={null} onReviewed={onReviewed} />, c));
    const button = (label: string) => [...c.querySelectorAll('button')].find((b) => b.textContent === label)!;
    button('Reject…').click();
    const input = c.querySelector('input')!;
    expect(button('Reject').disabled).toBe(true);
    input.value = 'too broad';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    expect(button('Reject').disabled).toBe(false);
    button('Reject').click();
    await flush();
    await flush();
    expect(calls[0][0]).toBe('/api/executions/req-1/attempts/1/review');
    expect(JSON.parse(String(calls[0][1]!.body))).toEqual({ verdict: 'rejected', reason: 'too broad' });
    expect(onReviewed).toHaveBeenCalledWith(expect.objectContaining({ verdict: 'rejected' }));
  });

  it('Accept sends no reason and Reject cannot be sent without one; the ids are stable', async () => {
    const calls: Array<unknown> = [];
    vi.spyOn(globalThis, 'fetch').mockImplementation((async (_input: RequestInfo | URL, init?: RequestInit) => {
      if (init?.body) calls.push(JSON.parse(String(init.body)));
      return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    }) as typeof fetch);
    const c = document.createElement('div');
    document.body.appendChild(c);
    disposers.push(render(() => <ReviewActions requestId="req-1" attemptNumber={1} review={null} />, c));
    const byId = (id: string) => c.querySelector<HTMLButtonElement>(`[data-testid="${id}"]`)!;
    byId('review-accept').click();
    await flush();
    await flush();
    byId('review-reject').click();
    expect(c.querySelector('input[data-testid="review-reason"]')).not.toBeNull();
    byId('review-reject').click();
    expect(byId('review-reject').disabled).toBe(true);
    const input = c.querySelector<HTMLInputElement>('input[data-testid="review-reason"]')!;
    input.value = 'wrong files';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    byId('review-reject').click();
    await flush();
    await flush();
    expect(calls).toEqual([{ verdict: 'accepted' }, { verdict: 'rejected', reason: 'wrong files' }]);
  });

  it('shows the verdict in place of the buttons once there is one', () => {
    const c = document.createElement('div');
    document.body.appendChild(c);
    disposers.push(
      render(
        () => <ReviewActions requestId="req-1" attemptNumber={1} review={{ verdict: 'accepted', reason: null, reviewed_by: null, reviewed_at: new Date().toISOString() }} />,
        c,
      ),
    );
    expect(c.textContent).toContain('Accepted');
    expect(c.querySelector('button')).toBeNull();
  });
});
