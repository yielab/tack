/**
 * Optimistic UI Update System
 *
 * Provides instant feedback by updating UI immediately, then rolling back on error.
 * Supports automatic retry and error recovery.
 */

import { createSignal, type Signal } from 'solid-js';
import { toast } from './toast';

export interface OptimisticUpdate<T> {
  id: string;
  originalValue: T;
  optimisticValue: T;
  timestamp: number;
}

export interface OptimisticOptions {
  /** Show success toast on completion (default: false) */
  showSuccessToast?: boolean;
  /** Success message for toast */
  successMessage?: string;
  /** Show error toast on failure (default: true) */
  showErrorToast?: boolean;
  /** Error message prefix for toast */
  errorMessage?: string;
  /** Auto-retry on failure (default: false) */
  autoRetry?: boolean;
  /** Max retry attempts (default: 3) */
  maxRetries?: number;
  /** Delay between retries in ms (default: 1000) */
  retryDelay?: number;
}

const DEFAULT_OPTIONS: Required<OptimisticOptions> = {
  showSuccessToast: false,
  successMessage: 'Operation completed',
  showErrorToast: true,
  errorMessage: 'Operation failed',
  autoRetry: false,
  maxRetries: 3,
  retryDelay: 1000,
};

/**
 * Execute an async operation with optimistic UI update
 *
 * @param operation - Async function to execute
 * @param optimisticUpdate - Function to apply optimistic update
 * @param rollback - Function to rollback on error
 * @param options - Configuration options
 *
 * @example
 * await withOptimisticUpdate(
 *   () => api.updateItem(id, { status: 'done' }),
 *   () => setItems(prev => prev.map(i => i.id === id ? {...i, status: 'done'} : i)),
 *   () => refetch(),
 *   { successMessage: 'Item updated' }
 * );
 */
export async function withOptimisticUpdate<T>(
  operation: () => Promise<T>,
  optimisticUpdate: () => void,
  rollback: () => void | Promise<void>,
  options: OptimisticOptions = {}
): Promise<T | null> {
  const opts = { ...DEFAULT_OPTIONS, ...options };
  let retries = 0;

  // Apply optimistic update immediately
  optimisticUpdate();

  const attemptOperation = async (): Promise<T | null> => {
    try {
      const result = await operation();

      if (opts.showSuccessToast && opts.successMessage) {
        toast.success(opts.successMessage);
      }

      return result;
    } catch (error) {
      // Retry logic
      if (opts.autoRetry && retries < opts.maxRetries) {
        retries++;
        await new Promise(resolve => setTimeout(resolve, opts.retryDelay));
        return attemptOperation();
      }

      // Rollback on failure
      await rollback();

      if (opts.showErrorToast) {
        const message = error instanceof Error ? error.message : opts.errorMessage;
        toast.error(message);
      }

      return null;
    }
  };

  return attemptOperation();
}

/**
 * Create a signal with optimistic update support
 *
 * Returns [getter, setter, optimisticUpdate] where optimisticUpdate
 * applies a change immediately and rolls back on error.
 */
export function createOptimisticSignal<T>(
  initialValue: T
): [() => T, (value: T) => void, (updater: (prev: T) => T, operation: () => Promise<any>, options?: OptimisticOptions) => Promise<void>] {
  const [value, setValue] = createSignal<T>(initialValue);

  const optimisticUpdate = async (
    updater: (prev: T) => T,
    operation: () => Promise<any>,
    options: OptimisticOptions = {}
  ) => {
    const originalValue = value();
    const optimisticValue = updater(originalValue);

    await withOptimisticUpdate(
      operation,
      () => setValue(optimisticValue as any),
      () => setValue(originalValue as any),
      options
    );
  };

  return [value, setValue, optimisticUpdate];
}

/**
 * Optimistic array operations
 */
export class OptimisticArray<T extends { id: string }> {
  private signal: Signal<T[]>;
  private setSignal: (value: T[]) => void;

  constructor(signal: Signal<T[]>, setSignal: (value: T[]) => void) {
    this.signal = signal;
    this.setSignal = setSignal;
  }

  /**
   * Optimistically add an item to the array
   */
  async add(
    item: T,
    operation: () => Promise<T>,
    options: OptimisticOptions = {}
  ): Promise<T | null> {
    const [items] = this.signal;
    const originalItems = items();

    return withOptimisticUpdate(
      operation,
      () => this.setSignal([...originalItems, item]),
      () => this.setSignal(originalItems),
      options
    );
  }

  /**
   * Optimistically update an item in the array
   */
  async update(
    id: string,
    updater: (item: T) => T,
    operation: () => Promise<T>,
    options: OptimisticOptions = {}
  ): Promise<T | null> {
    const [items] = this.signal;
    const originalItems = items();
    const optimisticItems = originalItems.map(item =>
      item.id === id ? updater(item) : item
    );

    return withOptimisticUpdate(
      operation,
      () => this.setSignal(optimisticItems),
      () => this.setSignal(originalItems),
      options
    );
  }

  /**
   * Optimistically remove an item from the array
   */
  async remove(
    id: string,
    operation: () => Promise<void>,
    options: OptimisticOptions = {}
  ): Promise<void> {
    const [items] = this.signal;
    const originalItems = items();
    const optimisticItems = originalItems.filter(item => item.id !== id);

    await withOptimisticUpdate(
      operation,
      () => this.setSignal(optimisticItems),
      () => this.setSignal(originalItems),
      options
    );
  }

  /**
   * Optimistically move an item to a different position
   */
  async move(
    _id: string, // Prefixed with _ to indicate intentionally unused
    fromIndex: number,
    toIndex: number,
    operation: () => Promise<void>,
    options: OptimisticOptions = {}
  ): Promise<void> {
    const [items] = this.signal;
    const originalItems = items();
    const optimisticItems = [...originalItems];
    const [movedItem] = optimisticItems.splice(fromIndex, 1);
    optimisticItems.splice(toIndex, 0, movedItem);

    await withOptimisticUpdate(
      operation,
      () => this.setSignal(optimisticItems),
      () => this.setSignal(originalItems),
      options
    );
  }
}

/**
 * Loading state management for async operations
 */
export function createLoadingState() {
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<Error | null>(null);

  const wrap = async <T,>(operation: () => Promise<T>): Promise<T | null> => {
    setLoading(true);
    setError(null);

    try {
      const result = await operation();
      return result;
    } catch (err) {
      setError(err instanceof Error ? err : new Error('Operation failed'));
      return null;
    } finally {
      setLoading(false);
    }
  };

  return { loading, error, wrap };
}

/**
 * Debounced optimistic update
 * Useful for search inputs, etc.
 */
export function createDebouncedOptimistic<T>(
  delay = 300
): (
  optimisticUpdate: () => void,
  operation: () => Promise<T>,
  rollback: () => void,
  options?: OptimisticOptions
) => Promise<void> {
  let timeoutId: number | undefined;

  return async (
    optimisticUpdate: () => void,
    operation: () => Promise<T>,
    rollback: () => void,
    options: OptimisticOptions = {}
  ) => {
    // Clear previous timeout
    if (timeoutId) {
      clearTimeout(timeoutId);
    }

    // Apply optimistic update immediately
    optimisticUpdate();

    // Debounce the actual operation
    return new Promise((resolve) => {
      timeoutId = window.setTimeout(async () => {
        await withOptimisticUpdate(
          operation,
          () => {}, // Already applied
          rollback,
          options
        );
        resolve();
      }, delay);
    });
  };
}
