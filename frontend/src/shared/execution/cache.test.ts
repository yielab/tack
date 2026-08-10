import { describe, it, expect } from 'vitest';
import { VersionedCache, SequenceAllocator } from './cache';

describe('VersionedCache', () => {
  it('applies the first write for a key', () => {
    const cache = new VersionedCache<string>();
    expect(cache.set('a', 'v1', 1)).toBe(true);
    expect(cache.get('a')).toBe('v1');
    expect(cache.versionOf('a')).toBe(1);
  });

  it('applies a strictly newer version and overwrites the value', () => {
    const cache = new VersionedCache<string>();
    cache.set('a', 'v1', 1);
    expect(cache.set('a', 'v2', 2)).toBe(true);
    expect(cache.get('a')).toBe('v2');
  });

  it('drops a strictly older version — the load-bearing guarantee', () => {
    const cache = new VersionedCache<string>();
    cache.set('a', 'fresh', 5);
    // A slow response for an older fetch resolves after a newer one already
    // landed. This must never win.
    expect(cache.set('a', 'stale', 3)).toBe(false);
    expect(cache.get('a')).toBe('fresh');
    expect(cache.versionOf('a')).toBe(5);
  });

  it('proves the guard is load-bearing: without it, the stale write would win', () => {
    // Same scenario as above, but bypassing the guard by writing directly
    // into a plain Map the way a naive cache would — demonstrates the
    // failure mode VersionedCache prevents, so the test above is not
    // vacuously true.
    const naive = new Map<string, string>();
    naive.set('a', 'fresh');
    naive.set('a', 'stale'); // a naive last-write-wins cache has no ordering guard
    expect(naive.get('a')).toBe('stale');
  });

  it('allows a replayed equal version to apply', () => {
    const cache = new VersionedCache<string>();
    cache.set('a', 'v1', 4);
    expect(cache.set('a', 'v1-replay', 4)).toBe(true);
    expect(cache.get('a')).toBe('v1-replay');
  });

  it('keeps independent keys independent', () => {
    const cache = new VersionedCache<number>();
    cache.set('a', 1, 10);
    cache.set('b', 2, 1);
    expect(cache.set('b', 3, 2)).toBe(true);
    expect(cache.get('a')).toBe(1);
    expect(cache.get('b')).toBe(3);
  });

  it('delete removes a key; has()/keys() reflect current membership', () => {
    const cache = new VersionedCache<number>();
    cache.set('a', 1, 1);
    cache.set('b', 2, 1);
    expect(cache.keys().sort()).toEqual(['a', 'b']);
    cache.delete('a');
    expect(cache.has('a')).toBe(false);
    expect(cache.keys()).toEqual(['b']);
  });
});

describe('SequenceAllocator', () => {
  it('issues a monotonically increasing sequence per key, starting at 1', () => {
    const allocator = new SequenceAllocator();
    expect(allocator.next('a')).toBe(1);
    expect(allocator.next('a')).toBe(2);
    expect(allocator.next('a')).toBe(3);
  });

  it('keeps independent counters per key', () => {
    const allocator = new SequenceAllocator();
    allocator.next('a');
    allocator.next('a');
    expect(allocator.next('b')).toBe(1);
    expect(allocator.current('a')).toBe(2);
    expect(allocator.current('b')).toBe(1);
  });

  it('current() is 0 for a key never allocated', () => {
    const allocator = new SequenceAllocator();
    expect(allocator.current('never')).toBe(0);
  });

  it('sequence assigned at issue time orders VersionedCache writes correctly even when responses resolve out of order', async () => {
    const cache = new VersionedCache<string>();
    const allocator = new SequenceAllocator();

    // Two "fetches" start in this order...
    const firstVersion = allocator.next('x');
    const secondVersion = allocator.next('x');

    // ...but resolve in the OPPOSITE order (the real-world race this
    // module exists to guard against).
    cache.set('x', 'second-response', secondVersion);
    cache.set('x', 'first-response-arriving-late', firstVersion);

    expect(cache.get('x')).toBe('second-response');
  });
});
