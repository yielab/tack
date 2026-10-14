import { type Component, Show, createSignal } from 'solid-js';
import { Button } from '../ui';
import { toast } from '../ui/toast';
import { ApiError } from '../api/client';
import { attemptsApi, type AttemptMerge } from '../execution/attempts';

export interface MergeButtonProps {
  attemptId: string;
  merge: AttemptMerge;
  /** Called after a merge went through, so the caller reads the attempt again. */
  onMerged?: () => void;
}

/**
 * Merges the attempt's pull request. Disabled with its reason when it cannot;
 * when it can, `merge.reason` is what held the last press. A refusal (409)
 * shows the server's words, which are also on the task.
 */
const MergeButton: Component<MergeButtonProps> = (props) => {
  const [busy, setBusy] = createSignal(false);
  const [refusal, setRefusal] = createSignal<string | null>(null);
  const press = async () => {
    if (busy() || !props.merge.enabled) return;
    setBusy(true);
    setRefusal(null);
    try {
      await attemptsApi.merge(props.attemptId);
      toast.success('Merged.');
      props.onMerged?.();
    } catch (err) {
      const message = err instanceof Error ? err.message : 'The merge did not go through';
      setRefusal(message);
      if (err instanceof ApiError && err.status === 409) props.onMerged?.();
      toast.error(message);
    } finally {
      setBusy(false);
    }
  };
  const note = () => refusal() ?? props.merge.reason ?? null;
  return (
    <div class="flex flex-wrap items-center gap-2" data-testid="merge-button">
      <Button type="button" size="sm" disabled={!props.merge.enabled || busy()} loading={busy()} onClick={() => void press()}>
        Merge
      </Button>
      <Show when={note()}>
        {(why) => (
          <span data-testid="merge-reason" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            {why()}
          </span>
        )}
      </Show>
    </div>
  );
};

export default MergeButton;
