// Unified API client foundation.
//
// Single place for base-URL resolution, auth header injection, and error
// handling. Every network call in the app must flow through one of the helpers
// exported here — no raw `fetch` and no absolute API hosts anywhere else.

/** Base URL for the API. Relative by default so the SPA works same-origin when
 * served from the `tack` binary or behind a reverse proxy. */
const BASE = import.meta.env.VITE_API_URL ?? '/api';

const TOKEN_STORAGE_KEY = 'tack_api_token';
const TOKEN_ORIGIN_KEY = 'tack_api_token_origin';

/** Parse the configured API base once at the network boundary. Credentials,
 * query strings and non-HTTP schemes are never valid API origins. */
export function apiBaseUrl(): URL {
  const base = new URL(BASE, window.location.href);
  if (
    !['http:', 'https:'].includes(base.protocol) ||
    base.username ||
    base.password ||
    base.search ||
    base.hash
  ) {
    throw new Error('VITE_API_URL must be a credential-free http(s) API base without query or fragment');
  }
  return base;
}

/** Origin used to scope an in-memory browser session credential. */
export function apiOrigin(): string {
  return apiBaseUrl().origin;
}

function scopedTokenKey(origin: string): string {
  return `${TOKEN_STORAGE_KEY}:${origin}`;
}

function browserSessionStorage(): Storage | null {
  try {
    return sessionStorage;
  } catch {
    return null;
  }
}

/**
 * Ensure an API token can never follow a changed API origin. The legacy
 * localStorage key is deleted rather than migrated: long-lived privileged
 * browser credentials are not part of the new session strategy.
 */
function scopedStorage(): { storage: Storage; key: string } | null {
  const storage = browserSessionStorage();
  if (!storage) return null;
  const origin = apiOrigin();
  const previousOrigin = storage.getItem(TOKEN_ORIGIN_KEY);
  if (previousOrigin && previousOrigin !== origin) {
    storage.removeItem(scopedTokenKey(previousOrigin));
  }
  storage.setItem(TOKEN_ORIGIN_KEY, origin);
  try {
    localStorage.removeItem(TOKEN_STORAGE_KEY);
  } catch {
    // A browser may disable persistent storage independently of sessionStorage.
  }
  return { storage, key: scopedTokenKey(origin) };
}

/**
 * Optional bearer token store. The backend can gate the API with
 * `TACK_API_TOKEN`; when set, every request must carry
 * `Authorization: Bearer <token>`. Tokens live only for the current browser
 * session and are bound to the configured API origin.
 */
export const tokenStore = {
  get(): string | null {
    const scoped = scopedStorage();
    return scoped?.storage.getItem(scoped.key) ?? null;
  },
  set(token: string | null): void {
    const scoped = scopedStorage();
    if (!scoped) return;
    if (token) scoped.storage.setItem(scoped.key, token);
    else scoped.storage.removeItem(scoped.key);
  },
};

/** Typed error carrying the HTTP status, the server's message text, and — when
 *  the server's error envelope includes one — a machine-readable `code`
 *  (`error.code` in the `{ "error": { status, message, code? } }` envelope).
 *  `code` is `undefined` for the large majority of errors, which still only
 *  carry `status`/`message`; callers that need to distinguish two failures
 *  sharing an HTTP status should check `code` first and fall back to
 *  `status` only when `code` is absent. */
export class ApiError extends Error {
  readonly status: number;
  readonly code?: string;

  constructor(status: number, message: string, code?: string) {
    super(message || `HTTP ${status}`);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
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
  let raw = '';
  try {
    raw = await res.text();
  } catch {
    /* body already consumed or unavailable */
  }

  // Preferred shape: the unified envelope `{ "error": { "status", "message",
  // "code"? } }`. `code` is optional — most errors don't carry one — and is
  // only ever a machine-readable string (never surfaced to the user
  // directly). Fall back to the raw body text (or status text) for non-JSON
  // error bodies so users never see raw JSON in a toast.
  let message = raw;
  let code: string | undefined;
  if (raw) {
    try {
      const parsed = JSON.parse(raw);
      const inner = parsed?.error;
      if (inner && typeof inner === 'object' && typeof inner.message === 'string') {
        message = inner.message;
        if (typeof inner.code === 'string') code = inner.code;
      } else if (typeof inner === 'string') {
        message = inner;
      }
    } catch {
      /* not JSON — keep the raw text */
    }
  }

  return new ApiError(res.status, message || res.statusText, code);
}

/**
 * Core JSON request helper.
 * - Sends `Content-Type: application/json` (overridable via `init.headers`).
 * - Attaches the bearer token when present.
 * - Throws {@link ApiError} on a non-2xx response.
 * - Returns `undefined` for `204 No Content`, otherwise the parsed JSON body.
 */
export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  return (await requestWithHeaders<T>(path, init)).data;
}

/** JSON request with response headers retained for conditional item edits. */
export async function requestWithHeaders<T>(
  path: string,
  init?: RequestInit,
): Promise<{ data: T; headers: Headers }> {
  const headers = authHeaders(init?.headers);
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }

  const res = await fetch(apiUrl(path), { ...init, headers, redirect: 'error' });

  if (!res.ok) throw await toApiError(res);
  if (res.status === 204) return { data: undefined as T, headers: res.headers };
  return { data: await res.json() as T, headers: res.headers };
}

/** Fetch a binary payload (downloads, exports, backups). */
export async function requestBlob(path: string, init?: RequestInit): Promise<Blob> {
  const headers = authHeaders(init?.headers);
  const res = await fetch(apiUrl(path), { ...init, headers, redirect: 'error' });
  if (!res.ok) throw await toApiError(res);
  return res.blob();
}

/**
 * Submit `multipart/form-data` (file uploads). Crucially does NOT set a
 * `Content-Type` header — the browser must add the multipart boundary itself.
 */
export async function requestForm<T>(path: string, form: FormData): Promise<T> {
  const headers = authHeaders();
  const res = await fetch(apiUrl(path), { method: 'POST', body: form, headers, redirect: 'error' });
  if (!res.ok) throw await toApiError(res);
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}
