import { type Component, Show, createSignal } from 'solid-js';
import { Button, type ButtonVariant } from '../ui';
import { attemptsApi, type AttemptSummary } from '../execution';
import { ITEM_UPDATED_EVENT } from '../state/itemEvents';
import { relativeTimeFromIso } from './shared';
import { Field } from '../ui';
import type { Mrp } from './MrpPanel';

export interface ReviewActionsProps {
  requestId: string;
  attemptNumber: number;
  /** The verdict as last read; once there is one, it is shown in place of the buttons. */
  review: AttemptSummary['review'] | undefined;
  /** Called with the saved verdict as the server now reports it. */
  onReviewed?: (review: AttemptSummary['review']) => void;
  /** How Accept is drawn; a screen whose one filled button is elsewhere passes `secondary`. */
  acceptVariant?: ButtonVariant;
  /** The run's pack. While it is on screen and not yet reviewed, one reason is required and is saved
   *  both on the pack's checks and with the run's verdict; without a pack the note is optional. */
  mrp?: Mrp;
}

/** Accept, or Reject… with an optional note, on one attempt; the verdict once it is given. */
const ReviewActions: Component<ReviewActionsProps> = (props) => {
  const [rejecting, setRejecting] = createSignal(false);
  const [note, setNote] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  // A pack that has no verdict yet is reviewed with the run: the reason is required, as the pack's review demands.
  const withPack = () => !!props.mrp && !!props.mrp.pack() && !props.mrp.review()?.verdict;
  // The pack was reviewed but the run was not (the second call failed, or the pack was reviewed on its own):
  // the pack's verdict and reason are what the run is given, and nothing is asked a second time.
  const packVerdict = () => (props.mrp?.pack() ? props.mrp.review() : null);
  const savedOnPackOnly = () => !!packVerdict()?.verdict;
  const reason = () => props.mrp?.reason().trim() ?? '';

  /** Records the verdict on the run, then reads it back. */
  const saveOnRun = async (verdict: 'accepted' | 'rejected', n: string) => {
    await attemptsApi.review(props.requestId, props.attemptNumber, { verdict, ...(n ? { note: n } : {}) });
    const list = await attemptsApi.list(props.requestId);
    const mine = list.data.data.find((a) => a.attempt_number === props.attemptNumber);
    props.onReviewed?.(mine?.review ?? null);
    window.dispatchEvent(new CustomEvent(ITEM_UPDATED_EVENT));
  };
  const submit = async (verdict: 'accepted' | 'rejected') => {
    // Read before the first call: once the pack's review is recorded, `withPack()` is false.
    const usePack = withPack();
    const why = reason();
    setBusy(true);
    setError(null);
    try {
      if (usePack) {
        await props.mrp!.submit(verdict === 'accepted' ? 'accept' : 'reject');
        // The pack's review reports its own failure as a toast and leaves no verdict; stop there.
        if (!props.mrp!.review()?.verdict) return;
      }
      // With a pack the one reason goes with either verdict; without, only a rejection may carry a note.
      await saveOnRun(verdict, usePack ? why : verdict === 'rejected' ? note().trim() : '');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'The verdict could not be saved');
    } finally {
      setBusy(false);
    }
  };
  const saveVerdict = async () => {
    const r = packVerdict();
    if (!r?.verdict) return;
    setBusy(true);
    setError(null);
    try {
      await saveOnRun(r.verdict === 'accept' ? 'accepted' : 'rejected', r.reason?.trim() ?? '');
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
        <Show when={savedOnPackOnly()}>
          <p class="w-full text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            The pack was {packVerdict()!.verdict === 'accept' ? 'accepted' : 'rejected'}
            {packVerdict()!.reason ? ` — ${packVerdict()!.reason}` : ''}, but the run has no verdict yet.
          </p>
          <Button size="sm" variant={props.acceptVariant} data-testid="review-save" disabled={busy()} onClick={() => void saveVerdict()}>
            Save the verdict on the run
          </Button>
        </Show>
        <Show when={!savedOnPackOnly()}>
          <Show when={withPack()}>
            <div class="w-full">
              <Field
                label="Reason"
                value={props.mrp!.reason()}
                onInput={(e) => props.mrp!.setReason(e.currentTarget.value)}
                required
                data-testid="review-reason"
              />
            </div>
          </Show>
          <Show
            when={rejecting() && !withPack()}
            fallback={
              <>
                <Button
                  size="sm"
                  variant={props.acceptVariant}
                  data-testid="review-accept"
                  disabled={busy() || (withPack() && !reason())}
                  onClick={() => void submit('accepted')}
                >
                  Accept
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  data-testid="review-reject"
                  disabled={busy() || (withPack() && !reason())}
                  onClick={() => (withPack() ? void submit('rejected') : setRejecting(true))}
                >
                  {withPack() ? 'Reject' : 'Reject…'}
                </Button>
                <Show when={withPack() && !reason()}>
                  <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                    Write a reason to enable Accept and Reject.
                  </span>
                </Show>
              </>
            }
          >
            <input
              type="text"
              aria-label="Why it is rejected (optional)"
              data-testid="review-reason"
              placeholder="Why (optional)"
              value={note()}
              onInput={(e) => setNote(e.currentTarget.value)}
              class="min-w-[10rem] flex-1 rounded-full px-3 py-1.5 text-sm"
              style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)', border: 'none' }}
            />
            <Button size="sm" variant="secondary" data-testid="review-reject" disabled={busy()} onClick={() => void submit('rejected')}>
              Reject
            </Button>
            <Button size="sm" variant="ghost" disabled={busy()} onClick={() => setRejecting(false)}>
              Back
            </Button>
          </Show>
        </Show>
        <Show when={error()}>
          <span class="text-xs" style={{ color: 'var(--color-danger-700)' }}>{error()}</span>
        </Show>
      </div>
    </Show>
  );
};

export default ReviewActions;
