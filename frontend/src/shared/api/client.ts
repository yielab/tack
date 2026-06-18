// Unified API client foundation (T-501).
//
// Single place for base-URL resolution, auth header injection, and error
// handling. Every network call in the app must flow through one of the helpers
// exported here — no raw `fetch` and no absolute API hosts anywhere else.

/** Base URL for the API. Relative by default so the SPA works same-origin when
 * served from the `tack-api` binary (T-403) or behind a reverse proxy. */
const BASE = import.meta.env.VITE_API_URL ?? '/api';

/**
 * Optional bearer token store. The backend can gate the API with
 * `TACK_API_TOKEN`; when set, every request must carry
 * `Authorization: Bearer <token>`. The token may be empty (no auth).
 */
export const tokenStore = {
  get(): string | null {
    // env override first, then persisted value; tolerate absent localStorage
    const fromEnv = import.meta.env.VITE_API_TOKEN as string | undefined;
    if (fromEnv) return fromEnv;
    try {
      return localStorage.getItem('tack_api_token');
    } catch {
      return null;
    }
  },
  set(token: string | null): void {
    try {
      if (token) localStorage.setItem('tack_api_token', token);
      else localStorage.removeItem('tack_api_token');
    } catch {
      /* ignore — localStorage may be unavailable */
    }
  },
};

/** Typed error carrying the HTTP status and the server's message text. */
export class ApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message || `HTTP ${status}`);
    this.name = 'ApiError';
    this.status = status;
  }
}

/** Join the configured base with a leading-slash path. */
export function apiUrl(path: string): string {
  return `${BASE}${path}`;
}

function authHeaders(extra?: HeadersInit): Headers {
  const headers = new Headers(extra);
  const token = tokenStore.get();
  if (token && !headers.has('Authorization')) {
    headers.set('Authorization', `Bearer ${token}`);
  }
  return headers;
}

async function toApiError(res: Response): Promise<ApiError> {
  let message = '';
  try {
    message = await res.text();
  } catch {
    /* body already consumed or unavailable */
  }
  return new ApiError(res.status, message || res.statusText);
}

/**
 * Core JSON request helper.
 * - Sends `Content-Type: application/json` (overridable via `init.headers`).
 * - Attaches the bearer token when present.
 * - Throws {@link ApiError} on a non-2xx response.
 * - Returns `undefined` for `204 No Content`, otherwise the parsed JSON body.
 */
export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = authHeaders(init?.headers);
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }

  const res = await fetch(apiUrl(path), { ...init, headers });

  if (!res.ok) throw await toApiError(res);
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

/** Fetch a binary payload (downloads, exports, backups). */
export async function requestBlob(path: string, init?: RequestInit): Promise<Blob> {
  const headers = authHeaders(init?.headers);
  const res = await fetch(apiUrl(path), { ...init, headers });
  if (!res.ok) throw await toApiError(res);
  return res.blob();
}

/**
 * Submit `multipart/form-data` (file uploads). Crucially does NOT set a
 * `Content-Type` header — the browser must add the multipart boundary itself.
 */
export async function requestForm<T>(path: string, form: FormData): Promise<T> {
  const headers = authHeaders();
  const res = await fetch(apiUrl(path), { method: 'POST', body: form, headers });
  if (!res.ok) throw await toApiError(res);
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}
