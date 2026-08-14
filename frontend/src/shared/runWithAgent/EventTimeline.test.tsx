import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import EventTimeline from './EventTimeline';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

function mount(requestId = 'exec_1', attemptNumber = 1) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <EventTimeline requestId={requestId} attemptNumber={attemptNumber} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('EventTimeline', () => {
  it('fetches GET /executions/{id}/attempts/{n}/events and renders events oldest-first with source/kind/payload', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [
            { event_id: 'evt_1', sequence: 1, source: 'harness', kind: 'message', payload: { text: 'Started' }, occurred_at: '2026-08-06T12:00:00Z', created_at: '2026-08-06T12:00:00Z' },
            { event_id: 'evt_2', sequence: 2, source: 'runner', kind: 'progress', payload: { phase: 'testing', percent: null }, occurred_at: '2026-08-06T12:01:00Z', created_at: '2026-08-06T12:01:00Z' },
          ],
        }),
        { status: 200 },
      ),
    );
    const c = mount('exec_1', 3);
    await flush();
    await flush();

    expect(String(fetchMock.mock.calls[0][0])).toBe('/api/executions/exec_1/attempts/3/events');
    expect(c.textContent).toContain('Started');
    expect(c.textContent).toContain('harness');
    expect(c.textContent).toContain('message');
    expect(c.textContent).toContain('runner');
    expect(c.textContent).toContain('progress');
  });

  it('shows an honest empty state for zero events, not a blank screen', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const c = mount();
    await flush();
    await flush();
    expect(c.textContent).toContain('No events reported yet');
  });

  it('a fetch failure is shown, not silently swallowed', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: { status: 500, message: 'boom' } }), { status: 500 }),
    );
    const c = mount();
    await flush();
    await flush();
    expect(c.textContent).toContain("Couldn't load events");
  });

  it('a payload without a text/message field falls back to compact JSON instead of throwing', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [{ event_id: 'evt_1', sequence: 1, source: 'runner', kind: 'custom', payload: { odd: 'shape', n: 1 }, occurred_at: '2026-08-06T12:00:00Z', created_at: '2026-08-06T12:00:00Z' }],
        }),
        { status: 200 },
      ),
    );
    const c = mount();
    await flush();
    await flush();
    expect(c.textContent).toContain('"odd":"shape"');
  });
});
