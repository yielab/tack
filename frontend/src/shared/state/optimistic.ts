/**
 * Optimistic UI Update System
 *
 * Provides instant feedback by updating UI immediately, then rolling back on error.
 * Supports automatic retry and error recovery.
 */

import { toast } from '../ui/toast';

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
 *   () => api.items.update(id, { status: 'done' }),
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
