import { beforeEach, vi } from 'vitest';

/**
 * Node's Fetch implementation and jsdom create Blobs in different realms.
 * Browser APIs accept either, so the test object-URL boundary must validate
 * the Blob protocol instead of relying on same-realm `instanceof`.
 */
function isBlobLike(value: unknown): value is Blob {
  if (value === null || typeof value !== 'object') return false;
  const candidate = value as Blob;
  return (
    Object.prototype.toString.call(value) === '[object Blob]' &&
    typeof candidate.size === 'number' &&
    typeof candidate.type === 'string' &&
    typeof candidate.arrayBuffer === 'function' &&
    typeof candidate.stream === 'function' &&
    typeof candidate.text === 'function'
  );
}

const createObjectURL = vi.fn((blob: Blob) => {
  if (!isBlobLike(blob)) throw new TypeError('createObjectURL requires a Blob');
  return `blob:vitest/${blob.size}`;
});
const revokeObjectURL = vi.fn((_url: string) => undefined);

Object.defineProperties(URL, {
  createObjectURL: { configurable: true, writable: true, value: createObjectURL },
  revokeObjectURL: { configurable: true, writable: true, value: revokeObjectURL },
});

beforeEach(() => {
  createObjectURL.mockClear();
  revokeObjectURL.mockClear();
});

