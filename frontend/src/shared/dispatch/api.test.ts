import { describe, it, expect, vi, afterEach } from 'vitest';
import { dispatchApi } from './api';

afterEach(() => {
  vi.restoreAllMocks();
});

describe('dispatchApi', () => {
  it('dispatchItem POSTs /items/{id}/dispatch with no body', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(
        new Response(JSON.stringify({ outcome: 'dispatched', task: null }), { status: 200 }),
      );

    const res = await dispatchApi.dispatchItem('item-1');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/items/item-1/dispatch');
    expect((init as RequestInit).method).toBe('POST');
    expect(res.outcome).toBe('dispatched');
  });

  it('dryRunSprintDispatch GETs /sprints/{id}/dispatch/dry-run with no query string when no cap is given', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(
        new Response(
          JSON.stringify({ sprint_id: 's1', max_in_flight: 5, summary: {}, items: [] }),
          { status: 200 },
        ),
      );

    const res = await dispatchApi.dryRunSprintDispatch('s1');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const url = String(fetchMock.mock.calls[0][0]);
    expect(url).toContain('/sprints/s1/dispatch/dry-run');
    expect(url).not.toContain('?');
    expect(res.max_in_flight).toBe(5);
  });

  it('dryRunSprintDispatch appends ?max_in_flight=N as a QUERY PARAM, never a JSON body — the real contract (card C3)', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(
        new Response(
          JSON.stringify({ sprint_id: 's1', max_in_flight: 3, summary: {}, items: [] }),
          { status: 200 },
        ),
      );

    await dispatchApi.dryRunSprintDispatch('s1', 3);

    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/sprints/s1/dispatch/dry-run?max_in_flight=3');
    expect((init as RequestInit | undefined)?.body).toBeUndefined();
  });

  it('dispatchSprint POSTs /sprints/{id}/dispatch with max_in_flight as a query param, no body', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ sprint_id: 's1', max_in_flight: 2, summary: {}, items: [] }), {
        status: 200,
      }),
    );

    await dispatchApi.dispatchSprint('s1', 2);

    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/sprints/s1/dispatch?max_in_flight=2');
    expect((init as RequestInit).method).toBe('POST');
    expect((init as RequestInit).body).toBeUndefined();
  });

  it('dispatchSprint omits the query string entirely when no override is given (server applies its own default)', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ sprint_id: 's1', max_in_flight: 5, summary: {}, items: [] }), {
        status: 200,
      }),
    );

    await dispatchApi.dispatchSprint('s1');

    const url = String(fetchMock.mock.calls[0][0]);
    expect(url).not.toContain('?');
  });
});
