import { type Component, For, Show, createResource, createSignal, type JSX } from 'solid-js';
import { Badge, Button, Field } from '../ui';
import type { BadgeTone } from '../ui';
import { toast } from '../ui/toast';
import { ApiError } from '../api/client';
import { mrpApi, type MrpCriterionStatus, type MrpReview } from '../execution/api';

export interface MrpPanelProps {
  requestId: string;
  attemptNumber: number;
}

const STATUS_TONE: Record<MrpCriterionStatus, BadgeTone> = {
  passed: 'success',
  failed: 'danger',
  manual: 'warning',
  skipped: 'neutral',
};

const RISK_TONE = { low: 'success', medium: 'warning', high: 'danger' } as const;

/** A section the verifier did not run. Plain text, never a tick. */
const NotRun: Component = () => (
  <p class="text-xs italic" style={{ color: 'var(--color-text-secondary)' }}>
    Not run
  </p>
);

const Section: Component<{ title: string; children: JSX.Element }> = (props) => (
  <section class="space-y-2">
    <h5 class="text-[11px] font-bold uppercase tracking-[0.1em]" style={{ color: 'var(--color-text-tertiary)' }}>
      {props.title}
    </h5>
    {props.children}
  </section>
);

const mono = { 'font-family': 'var(--font-mono)' } as const;

/**
 * The verifier's merge-readiness pack for one attempt, with the accept /
 * reject verdict. A section the verifier did not run says "Not run".
 */
const MrpPanel: Component<MrpPanelProps> = (props) => {
  const [data, { refetch }] = createResource(
    () => `${props.requestId}:${props.attemptNumber}`,
    () => mrpApi.get(props.requestId, props.attemptNumber),
  );
  const [reviewed, setReviewed] = createSignal<MrpReview | null>(null);
  const [reason, setReason] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  // One `viewed` call for as long as the panel is mounted.
  let viewedSent = false;
  const review = () => reviewed() ?? data()?.review ?? null;
  const markViewedOnce = () => {
    const r = data();
    if (!r || viewedSent || r.review?.viewed_at || r.review?.verdict) return;
    viewedSent = true;
    mrpApi.markViewed(props.requestId, props.attemptNumber).catch(() => {
      /* best effort */
    });
  };

  const submit = async (verdict: 'accept' | 'reject') => {
    if (!reason().trim() || busy()) return;
    setBusy(true);
    try {
      setReviewed(await mrpApi.review(props.requestId, props.attemptNumber, verdict, reason().trim()));
      toast.success(verdict === 'accept' ? 'Pack accepted.' : 'Pack rejected.');
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        toast.error('This pack was already reviewed.');
        void refetch();
      } else {
        toast.error(err instanceof Error ? err.message : 'Could not save the review.');
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="space-y-4">
      <Show when={data.loading}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading the pack…
        </p>
      </Show>
      <Show when={data.error}>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load the pack: {data.error instanceof Error ? data.error.message : 'unknown error'}
        </p>
      </Show>
      <Show when={data()}>
        {(resp) => {
          markViewedOnce();
          const pack = () => resp().pack;
          return (
            <>
              <div class="space-y-1">
                <div class="flex flex-wrap items-center gap-2">
                  <Badge tone={pack().recommendation.decision === 'merge' ? 'success' : pack().recommendation.decision === 'reject' ? 'danger' : 'warning'}>
                    Recommendation: {pack().recommendation.decision}
                  </Badge>
                  <Badge tone={RISK_TONE[pack().risk.tier] ?? 'neutral'}>Risk: {pack().risk.tier}</Badge>
                </div>
                <p class="text-sm" style={{ color: 'var(--color-text-primary)' }}>
                  {pack().recommendation.rationale}
                </p>
                <Show when={pack().risk.reasons.length > 0}>
                  <ul class="list-disc pl-5 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                    <For each={pack().risk.reasons}>{(r) => <li>{r}</li>}</For>
                  </ul>
                </Show>
              </div>

              <Section title="Criteria">
                <Show when={pack().criteria.length > 0} fallback={<NotRun />}>
                  <table class="w-full text-left text-xs" style={{ color: 'var(--color-text-primary)' }}>
                    <thead style={{ color: 'var(--color-text-tertiary)' }}>
                      <tr>
                        <th class="pr-3 font-semibold">Criterion</th>
                        <th class="pr-3 font-semibold">Kind</th>
                        <th class="pr-3 font-semibold">Status</th>
                        <th class="pr-3 font-semibold">Evidence</th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={pack().criteria}>
                        {(c) => (
                          <tr data-testid="mrp-criterion">
                            <td class="py-1 pr-3 align-top">
                              <span style={mono}>{c.id}</span>
                              <Show when={c.note}>
                                <span class="block" style={{ color: 'var(--color-text-secondary)' }}>{c.note}</span>
                              </Show>
                            </td>
                            <td class="py-1 pr-3 align-top">{c.kind}</td>
                            <td class="py-1 pr-3 align-top">
                              <Badge tone={STATUS_TONE[c.status] ?? 'neutral'}>{c.status}</Badge>
                            </td>
                            <td class="py-1 pr-3 align-top break-all" style={mono}>{c.evidence_ref ?? 'none'}</td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </Show>
              </Section>

              <Section title="Verify">
                <Show when={pack().verify} fallback={<NotRun />}>
                  {(v) => (
                    <div class="space-y-1 text-xs">
                      <p style={mono}>{v().command}</p>
                      <p>
                        Exit code <span style={mono}>{v().exit_code}</span>
                      </p>
                      <Show when={v().output_tail}>
                        <pre class="overflow-x-auto rounded-[12px] p-2" style={{ ...mono, 'background-color': 'var(--color-bg-panel)' }}>
                          {v().output_tail}
                        </pre>
                      </Show>
                    </div>
                  )}
                </Show>
              </Section>

              <Section title="Mutation">
                <Show when={pack().mutation} fallback={<NotRun />}>
                  {(m) => (
                    <p class="text-xs">
                      {m().score}% on {m().scope}: {m().killed} killed, {m().survived} survived
                    </p>
                  )}
                </Show>
              </Section>

              <Section title="Static analysis">
                <Show when={pack().static_analysis} fallback={<NotRun />}>
                  {(s) => (
                    <p class="text-xs">
                      {s().counts.error} errors, {s().counts.warning} warnings, {s().counts.note} notes
                    </p>
                  )}
                </Show>
              </Section>

              <Section title="Judge">
                <Show when={pack().judge} fallback={<NotRun />}>
                  {(j) => (
                    <div class="space-y-1 text-xs">
                      <p style={{ color: 'var(--color-text-secondary)' }}>
                        {j().model_family}
                        {j().blind ? ' (blind)' : ''}
                      </p>
                      <ul class="space-y-1">
                        <For each={j().rubric}>
                          {(e) => (
                            <li>
                              <span style={mono}>{e.criterion_id}</span>{' '}
                              <Badge tone={e.verdict === 'pass' ? 'success' : 'danger'}>{e.verdict}</Badge>
                              <Show when={e.reason}>
                                <span style={{ color: 'var(--color-text-secondary)' }}> {e.reason}</span>
                              </Show>
                            </li>
                          )}
                        </For>
                      </ul>
                    </div>
                  )}
                </Show>
              </Section>

              <Section title="Your review">
                <Show
                  when={review()?.verdict}
                  fallback={
                    <form
                      class="space-y-3"
                      onSubmit={(e) => {
                        e.preventDefault();
                      }}
                    >
                      <Field
                        label="Reason"
                        value={reason()}
                        onInput={(e) => setReason(e.currentTarget.value)}
                        required
                      />
                      <div class="flex flex-wrap items-center gap-2">
                        <Button type="button" size="sm" disabled={busy() || !reason().trim()} onClick={() => submit('accept')}>
                          Accept
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          variant="secondary"
                          disabled={busy() || !reason().trim()}
                          onClick={() => submit('reject')}
                        >
                          Reject
                        </Button>
                        <Show when={!reason().trim()}>
                          <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                            Write a reason to enable Accept and Reject.
                          </span>
                        </Show>
                      </div>
                    </form>
                  }
                >
                  <p class="text-sm" data-testid="mrp-review">
                    <Badge tone={review()?.verdict === 'accept' ? 'success' : 'danger'}>
                      {review()?.verdict === 'accept' ? 'Accepted' : 'Rejected'}
                    </Badge>{' '}
                    {review()?.reason}
                  </p>
                </Show>
              </Section>
            </>
          );
        }}
      </Show>
    </div>
  );
};

export default MrpPanel;
