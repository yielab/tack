import { describe, it, expect, vi, beforeEach } from 'vitest';
import { withOptimisticUpdate } from './optimistic';

// Prevent solid-js signal calls inside toast from erroring in a non-reactive context.
vi.mock('../ui/toast', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
  },
}));

import { toast } from '../ui/toast';

beforeEach(() => {
  vi.clearAllMocks();
});

describe('withOptimisticUpdate', () => {
  it('applies the optimistic update immediately before the operation resolves', async () => {
    const order: string[] = [];
    const optimistic = vi.fn(() => order.push('optimistic'));
    const operation = vi.fn(async () => {
      order.push('operation');
      return 'result';
    });
    const rollback = vi.fn();

    await withOptimisticUpdate(operation, optimistic, rollback);

    expect(order).toEqual(['optimistic', 'operation']);
  });

  it('returns the operation result on success', async () => {
    const result = await withOptimisticUpdate(
      async () => 'value',
      vi.fn(),
      vi.fn(),
    );
    expect(result).toBe('value');
  });

  it('does NOT call rollback when the operation succeeds', async () => {
    const rollback = vi.fn();
    await withOptimisticUpdate(async () => 42, vi.fn(), rollback);
    expect(rollback).not.toHaveBeenCalled();
  });

  it('calls rollback when the operation throws', async () => {
    const rollback = vi.fn();
    await withOptimisticUpdate(
      async () => { throw new Error('oops'); },
      vi.fn(),
      rollback,
    );
    expect(rollback).toHaveBeenCalledOnce();
  });

  it('returns null when the operation throws', async () => {
    const result = await withOptimisticUpdate(
      async () => { throw new Error('fail'); },
      vi.fn(),
      vi.fn(),
    );
    expect(result).toBeNull();
  });

  it('shows an error toast by default when the operation fails', async () => {
    await withOptimisticUpdate(
      async () => { throw new Error('boom'); },
      vi.fn(),
      vi.fn(),
    );
    expect(toast.error).toHaveBeenCalledOnce();
  });

  it('suppresses the error toast when showErrorToast is false', async () => {
    await withOptimisticUpdate(
      async () => { throw new Error('silent'); },
      vi.fn(),
      vi.fn(),
      { showErrorToast: false },
    );
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('shows a success toast when showSuccessToast is true', async () => {
    await withOptimisticUpdate(
      async () => 'ok',
      vi.fn(),
      vi.fn(),
      { showSuccessToast: true, successMessage: 'Done!' },
    );
    expect(toast.success).toHaveBeenCalledWith('Done!');
  });

  it('does NOT show a success toast by default', async () => {
    await withOptimisticUpdate(async () => 'ok', vi.fn(), vi.fn());
    expect(toast.success).not.toHaveBeenCalled();
  });

  it('uses the Error message in the error toast over the default errorMessage', async () => {
    await withOptimisticUpdate(
      async () => { throw new Error('specific error'); },
      vi.fn(),
      vi.fn(),
    );
    expect(toast.error).toHaveBeenCalledWith('specific error');
  });
});
