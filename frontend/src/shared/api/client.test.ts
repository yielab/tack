import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  request,
  requestBlob,
  requestForm,
  ApiError,
  isSessionEnded,
  tokenStore,
} from './client';

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
}

describe('shared/api/client', () => {
  beforeEach(() => {
    tokenStore.set(null);
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('joins the default base (/api) with the path', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ ok: true }));

    await request('/projects');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/projects');
  });

  it('sends Content-Type: application/json by default', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({}));

    await request('/projects', { method: 'POST', body: '{}' });

    const headers = fetchMock.mock.calls[0][1]!.headers as Headers;
    expect(headers.get('Content-Type')).toBe('application/json');
  });

  it('returns the parsed JSON body on 2xx', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ id: '42' })
    );
    await expect(request<{ id: string }>('/projects/42')).resolves.toEqual({
      id: '42',
    });
  });

  it('returns undefined for 204 No Content', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(null, { status: 204 })
    );
    await expect(request('/projects/1', { method: 'DELETE' })).resolves.toBeUndefined();
  });

  it('throws ApiError carrying status + message on non-2xx', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('cycle detected', { status: 400 })
    );

    const err = (await request('/items/1/dependencies', { method: 'POST' }).catch(
      (e) => e
    )) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(400);
    expect(err.message).toBe('cycle detected');
  });

  it('extracts the message from the structured error envelope', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: { status: 409, message: 'remote is ahead' } }), {
        status: 409,
        headers: { 'Content-Type': 'application/json' },
      })
    );

    const err = (await request('/backup/remote', { method: 'POST' }).catch(
      (e) => e
    )) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(409);
    // The user sees the human message, not the raw JSON body.
    expect(err.message).toBe('remote is ahead');
  });

  it('extracts an optional error.code from the structured envelope', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          error: { status: 409, message: 'Orchestration is disabled', code: 'orchestration_disabled' },
        }),
        { status: 409, headers: { 'Content-Type': 'application/json' } }
      )
    );

    const err = (await request('/fleet').catch((e) => e)) as ApiError;
    expect(err.status).toBe(409);
    expect(err.code).toBe('orchestration_disabled');
  });

  it('extracts error.details.field when it is a string', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: { status: 400, message: 'bad', details: { field: 'repository' } } }), {
        status: 400,
        headers: { 'Content-Type': 'application/json' },
      })
    );
    const err = (await request('/projects/1', { method: 'PATCH' }).catch((e) => e)) as ApiError;
    expect(err.field).toBe('repository');
  });

  it('leaves code undefined when the envelope has none', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: { status: 404, message: 'not found' } }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      })
    );

    const err = (await request('/items/1').catch((e) => e)) as ApiError;
    expect(err.code).toBeUndefined();
  });

  it('attaches a bearer token from the token store when present', async () => {
    vi.spyOn(tokenStore, 'get').mockReturnValue('secret');
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({}));

    await request('/projects');

    const headers = fetchMock.mock.calls[0][1]!.headers as Headers;
    expect(headers.get('Authorization')).toBe('Bearer secret');
    expect(fetchMock.mock.calls[0][1]!.credentials).toBe('same-origin');
  });

  it('keeps a bearer token only in the current session', () => {
    tokenStore.set('session-secret');

    expect(tokenStore.get()).toBe('session-secret');
    expect(localStorage.getItem('tack_api_token')).toBeNull();
    expect(sessionStorage.getItem(`tack_api_token:${location.origin}`)).toBe('session-secret');
  });

  it('omits Authorization when no token is set', async () => {
    vi.spyOn(tokenStore, 'get').mockReturnValue(null);
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({}));

    await request('/projects');

    const headers = fetchMock.mock.calls[0][1]!.headers as Headers;
    expect(headers.has('Authorization')).toBe(false);
    expect(fetchMock.mock.calls[0][1]!.credentials).toBe('same-origin');
  });

  it('requestBlob returns a Blob and throws ApiError on failure', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('binary', { status: 200 })
    );
    const blob = await requestBlob('/backup');
    // Fetch and jsdom may construct Blobs in different realms. Assert the
    // complete browser Blob protocol and payload instead of same-realm
    // `instanceof`, which rejects a valid cross-realm Blob.
    expect(Object.prototype.toString.call(blob)).toBe('[object Blob]');
    expect(blob).toMatchObject({ size: 6, type: 'text/plain;charset=utf-8' });
    await expect(blob.text()).resolves.toBe('binary');

    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('nope', { status: 500 })
    );
    await expect(requestBlob('/backup')).rejects.toBeInstanceOf(ApiError);
  });

  it('requestForm does NOT set a Content-Type header (browser adds boundary)', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ id: 'a' }));

    const form = new FormData();
    form.append('file', new Blob(['x']), 'x.txt');
    await requestForm('/items/1/attachments', form);

    const headers = fetchMock.mock.calls[0][1]!.headers as Headers;
    expect(headers.has('Content-Type')).toBe(false);
    expect((fetchMock.mock.calls[0][1] as RequestInit).redirect).toBe('error');
  });

  describe('an ended session', () => {
    const unauthorized = () =>
      new Response(JSON.stringify({ error: { status: 401, message: 'Unauthorized', code: 'unauthorized' } }), { status: 401 });
    const page = () => new Response('<!doctype html>', { status: 200, headers: { 'Content-Type': 'text/html' } });

    it('loads the page in the background to renew the cookie, then sends the call again', async () => {
      const fetchMock = vi
        .spyOn(globalThis, 'fetch')
        .mockResolvedValueOnce(unauthorized())
        .mockResolvedValueOnce(page())
        .mockResolvedValueOnce(jsonResponse({ id: '7' }));

      await expect(request<{ id: string }>('/projects/7')).resolves.toEqual({ id: '7' });

      expect(fetchMock).toHaveBeenCalledTimes(3);
      const renewal = fetchMock.mock.calls[1];
      expect(String(renewal[0])).toBe(new URL('/', window.location.href).toString());
      expect((renewal[1] as RequestInit).redirect).toBe('manual');
      expect(fetchMock.mock.calls[2][0]).toBe('/api/projects/7');
    });

    it('says the session ended when the call is still refused after the renewal', async () => {
      vi.spyOn(globalThis, 'fetch')
        .mockResolvedValueOnce(unauthorized())
        .mockResolvedValueOnce(page())
        .mockResolvedValueOnce(unauthorized());

      const error = await request('/projects').catch((e: unknown) => e);
      expect(isSessionEnded(error)).toBe(true);
      expect((error as ApiError).status).toBe(401);
      expect((error as ApiError).message).toBe('Your session ended. Reload the page to sign in again.');
    });

    it('does not send the call again when the page could not be loaded (a sign-in in front of the board)', async () => {
      const fetchMock = vi
        .spyOn(globalThis, 'fetch')
        .mockResolvedValueOnce(unauthorized())
        .mockResolvedValueOnce(new Response(null, { status: 302 }));

      const error = await request('/projects').catch((e: unknown) => e);
      expect(isSessionEnded(error)).toBe(true);
      expect(fetchMock).toHaveBeenCalledTimes(2);
    });

    it('never renews for a page that holds its own token', async () => {
      tokenStore.set('secret-token');
      const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(unauthorized());

      const error = await request('/projects').catch((e: unknown) => e);
      expect(isSessionEnded(error)).toBe(true);
      expect(fetchMock).toHaveBeenCalledTimes(1);
    });
  });
});
