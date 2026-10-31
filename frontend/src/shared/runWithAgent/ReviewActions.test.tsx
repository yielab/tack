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
  it('Reject… asks for an optional reason before it sends the verdict', async () => {
    const calls: Array<[string, RequestInit | undefined]> = [];
    vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
      calls.push([String(input), init]);
      const review = { verdict: 'rejected', note: 'too broad', reviewed_at: '2026-10-19T10:00:00Z' };
      return new Response(JSON.stringify({ protocol_version: 1, data: [{ attempt_number: 1, review }] }), { status: 200 });
    }) as typeof fetch);
    const onReviewed = vi.fn();
    const c = document.createElement('div');
    document.body.appendChild(c);
    disposers.push(render(() => <ReviewActions requestId="req-1" attemptNumber={1} review={null} onReviewed={onReviewed} />, c));
    const button = (label: string) => [...c.querySelectorAll('button')].find((b) => b.textContent === label)!;
    button('Reject…').click();
    const input = c.querySelector('input')!;
    input.value = 'too broad';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    button('Reject').click();
    await flush();
    await flush();
    expect(calls[0][0]).toBe('/api/executions/req-1/attempts/1/review');
    expect(JSON.parse(String(calls[0][1]!.body))).toEqual({ verdict: 'rejected', note: 'too broad' });
    expect(onReviewed).toHaveBeenCalledWith(expect.objectContaining({ verdict: 'rejected' }));
  });

  it('shows the verdict in place of the buttons once there is one', () => {
    const c = document.createElement('div');
    document.body.appendChild(c);
    disposers.push(
      render(
        () => <ReviewActions requestId="req-1" attemptNumber={1} review={{ verdict: 'accepted', note: null, reviewed_at: new Date().toISOString() }} />,
        c,
      ),
    );
    expect(c.textContent).toContain('Accepted');
    expect(c.querySelector('button')).toBeNull();
  });
});
