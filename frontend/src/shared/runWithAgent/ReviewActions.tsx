import { type Component, Show, createSignal } from 'solid-js';
import { Button } from '../ui';
import { attemptsApi, type AttemptSummary } from '../execution';
import { ITEM_UPDATED_EVENT } from '../state/itemEvents';
import { relativeTimeFromIso } from './shared';

export interface ReviewActionsProps {
  requestId: string;
  attemptNumber: number;
  /** The verdict as last read; once there is one, it is shown in place of the buttons. */
  review: AttemptSummary['review'] | undefined;
  /** Called with the saved verdict as the server now reports it. */
  onReviewed?: (review: AttemptSummary['review']) => void;
}

/** Accept, or Reject… with an optional note, on one attempt; the verdict once it is given. */
const ReviewActions: Component<ReviewActionsProps> = (props) => {
  const [rejecting, setRejecting] = createSignal(false);
  const [note, setNote] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const submit = async (verdict: 'accepted' | 'rejected') => {
    setBusy(true);
    setError(null);
    try {
      const n = verdict === 'rejected' ? note().trim() : '';
      await attemptsApi.review(props.requestId, props.attemptNumber, { verdict, ...(n ? { note: n } : {}) });
      const list = await attemptsApi.list(props.requestId);
      const mine = list.data.data.find((a) => a.attempt_number === props.attemptNumber);
      props.onReviewed?.(mine?.review ?? null);
      window.dispatchEvent(new CustomEvent(ITEM_UPDATED_EVENT));
    } catch (e) {
      setError(e instanceof Error ? e.message : 'The verdict could not be saved');
    } finally {
      setBusy(false);
    }
  };
  return (
    <Show
      when={!props.review}
      fallback={
        <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          {props.review!.verdict === 'accepted' ? 'Accepted' : 'Rejected'} {relativeTimeFromIso(props.review!.reviewed_at)}
          {props.review!.note ? ` — ${props.review!.note}` : ''}
        </p>
      }
    >
      <div data-testid="review-actions" class="flex flex-wrap items-center gap-2">
        <Show
          when={rejecting()}
          fallback={
            <>
              <Button size="sm" disabled={busy()} onClick={() => void submit('accepted')}>
                Accept
              </Button>
              <Button size="sm" variant="secondary" disabled={busy()} onClick={() => setRejecting(true)}>
                Reject…
              </Button>
            </>
          }
        >
          <input
            type="text"
            aria-label="Why it is rejected (optional)"
            placeholder="Why (optional)"
            value={note()}
            onInput={(e) => setNote(e.currentTarget.value)}
            class="min-w-[10rem] flex-1 rounded-full px-3 py-1.5 text-sm"
            style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)', border: 'none' }}
          />
          <Button size="sm" variant="secondary" disabled={busy()} onClick={() => void submit('rejected')}>
            Reject
          </Button>
          <Button size="sm" variant="ghost" disabled={busy()} onClick={() => setRejecting(false)}>
            Back
          </Button>
        </Show>
        <Show when={error()}>
          <span class="text-xs" style={{ color: 'var(--color-danger-700)' }}>{error()}</span>
        </Show>
      </div>
    </Show>
  );
};

export default ReviewActions;
