import { type Component, For, Show, createEffect, createMemo, createResource, createSignal, type JSX } from 'solid-js';
import { Badge, Button, Field, RunState } from '../ui';
import { toast } from '../ui/toast';
import { ApiError } from '../api/client';
import type { AcceptanceCriterion } from '../api/briefs';
import { mrpApi, type MrpCriterionStatus, type MrpPack, type MrpReview } from '../execution/api';
import type { AttemptMerge } from '../execution/attempts';
import { LEVEL_MEANING } from '../help/texts';
import { plainLine } from './plainWords';

export interface MrpPanelProps {
  /** The run's verdict is asked for at the top of the run, so this panel only shows it once given. */
  verdictAbove?: boolean;
  mrp: Mrp;
  /** The item's "Done when" list: its ticks and the criteria the verifier did not name sit in the same checklist. */
  doneWhen?: AcceptanceCriterion[] | null;
}

/** A check's state as a glyph and its words: the glyph is never the only carrier, and the colour only repeats it. */
const CHECK_MARK: Record<MrpCriterionStatus, { glyph: string; label: string; color: string }> = {
  passed: { glyph: '✓', label: 'Passed', color: 'var(--color-success-600)' },
  failed: { glyph: '✗', label: 'Failed', color: 'var(--color-danger-600)' },
  manual: { glyph: '◐', label: 'Needs a person', color: 'var(--color-warning-600)' },
  skipped: { glyph: '–', label: 'Skipped', color: 'var(--color-text-tertiary)' },
};

/** One line of the checklist: where it comes from decides how it is read. */
export interface CheckItem {
  id: string;
  title: string;
  status: MrpCriterionStatus;
  kind: string;
  /** `Done when`: a line the item's own list has for a person to check. `Agent`: any other check, the ones the agent brings. */
  source: 'Done when' | 'Agent';
  evidence: string | null;
  note: string | null;
  fromPack: boolean;
}

/**
 * The verifier's criteria and the item's "Done when" list as one checklist. A criterion the verifier
 * reports under an id the list also has takes the list's title and, for a manual one a person ticked,
 * counts as passed; a criterion the verifier never reported stays on the list, ticked or not.
 */
export function buildChecks(pack: MrpPack | undefined, doneWhen: AcceptanceCriterion[] | null | undefined): CheckItem[] {
  const listed = new Map((doneWhen ?? []).map((c) => [c.id, c]));
  const items: CheckItem[] = [];
  const seen = new Set<string>();
  for (const c of pack?.criteria ?? []) {
    const own = listed.get(c.id);
    seen.add(c.id);
    const ticked = own?.kind === 'manual' && own.checked === true;
    items.push({
      id: c.id,
      title: own?.title?.trim() || c.id,
      status: ticked ? 'passed' : c.status,
      kind: c.kind,
      source: own?.kind === 'manual' ? 'Done when' : 'Agent',
      evidence: c.evidence_ref,
      note: c.note ?? (ticked ? 'Ticked by a person' : null),
      fromPack: true,
    });
  }
  for (const own of doneWhen ?? []) {
    if (seen.has(own.id)) continue;
    const manual = own.kind === 'manual';
    const ticked = manual && own.checked === true;
    items.push({
      id: own.id,
      title: own.title.trim() || own.id,
      status: manual ? (ticked ? 'passed' : 'manual') : 'skipped',
      kind: own.kind,
      source: manual ? 'Done when' : 'Agent',
      evidence: null,
      note: ticked ? 'Ticked by a person' : manual ? null : 'The verifier did not run it',
      fromPack: false,
    });
  }
  return items;
}

/** The checklist in one sentence with its number. */
export function checksSummary(items: CheckItem[]): string {
  if (items.length === 0) return 'No checks were recorded for this run.';
  const passed = items.filter((i) => i.status === 'passed').length;
  const failed = items.filter((i) => i.status === 'failed').length;
  const open = items.filter((i) => i.status === 'manual').length;
  return [`${passed} of ${items.length} checks passed`, failed > 0 ? `${failed} failed` : null, open > 0 ? `${open} need a person` : null]
    .filter(Boolean)
    .join(' · ');
}

/** The verifier's next-step codes in words (a code it does not know is shown as it came), and its severities. */
const CODE_WORDS: Record<string, string> = {
  criterion_manual: 'A criterion needs a person to check it.',
  criterion_failed: 'A criterion failed.',
  test_failed: 'A test failed.',
  coverage_below_threshold: 'Coverage is below the threshold.',
  stage_degraded: 'A verifier stage ran in a reduced way.',
};
const SEVERITY_WORDS: Record<string, string> = { error: 'Problem', warning: 'Warning', review: 'Needs a look' };

const RISK_TONE = { low: 'success', medium: 'warning', high: 'danger' } as const;

/** A section the verifier did not run. Plain text, never a tick. */
const NotRun: Component = () => (
  <p class="text-xs italic" style={{ color: 'var(--color-text-secondary)' }}>
    Not run
  </p>
);

const Section: Component<{ title: string; children: JSX.Element }> = (props) => (
  <section class="space-y-2">
    <h5 class="text-micro font-bold uppercase tracking-[0.1em]" style={{ color: 'var(--color-text-tertiary)' }}>
      {props.title}
    </h5>
    {props.children}
  </section>
);

/** "Merged by Tack (L4, domain docs)" or "Merged by you"; the policy behind it is a disclosure. */
export const MergeState: Component<{ merge: AttemptMerge }> = (props) => {
  const label = () => {
    const m = props.merge.merged;
    if (!m) return null;
    if (m.trigger === 'button') return 'Merged by you';
    const parts = [m.level, m.domain ? `domain ${m.domain}` : null].filter(Boolean);
    return `Merged by Tack${parts.length > 0 ? ` (${parts.join(', ')})` : ''}`;
  };
  const levelSentence = () => {
    const m = props.merge.merged;
    return m && m.trigger !== 'button' && m.level && m.level in LEVEL_MEANING ? LEVEL_MEANING[m.level as keyof typeof LEVEL_MEANING] : null;
  };
  return (
    <Show when={label() ?? props.merge.reason}>
      {(text) => (
        <div data-testid="mrp-merge-state" class="space-y-1 text-sm" style={{ color: 'var(--color-text-primary)' }}>
          <p>
            <Show when={!props.merge.merged}>
              <span class="font-semibold">Not merged: </span>
            </Show>
            {text()}
          </p>
          <Show when={levelSentence()}>
            {(sentence) => (
              <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>{sentence()}</p>
            )}
          </Show>
          <Show when={props.merge.merged?.policy_snapshot}>
            {(policy) => (
              <details class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                <summary class="cursor-pointer">The rules it was merged under</summary>
                <pre class="overflow-x-auto rounded-[12px] p-2" style={{ ...mono, 'background-color': 'var(--color-bg-panel)' }}>
                  {JSON.stringify(policy(), null, 2)}
                </pre>
              </details>
            )}
          </Show>
        </div>
      )}
    </Show>
  );
};

const mono = { 'font-family': 'var(--font-mono)' } as const;

/** Whether the verifier signed the pack, in plain words; the pack names no key, only what was signed. */
export const SignatureState: Component<{ signature: MrpPack['signature'] | undefined }> = (props) => (
  <p data-testid="mrp-signature" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
    <Show
      when={props.signature?.kind === 'in-toto' ? props.signature : null}
      fallback={<>Not signed: the verifier put no signature on this pack.</>}
    >
      {(signed) => (
        <>
          Signed by the verifier. What it signed has the fingerprint{' '}
          <span style={mono}>{signed().statement_sha256.slice(0, 12)}</span>.
        </>
      )}
    </Show>
  </p>
);

/** Where the pack for one attempt is read and reviewed. Any part of the run's screen can take what
 *  it shows from it; nothing is fetched until `source` names the attempt (a run with no pack names none). */
export function createMrp(source: () => { requestId: string; attemptNumber: number } | undefined) {
  const which = createMemo(source, undefined, {
    equals: (a, b) => a?.requestId === b?.requestId && a?.attemptNumber === b?.attemptNumber,
  });
  const [data, { refetch }] = createResource(which, (s) => mrpApi.get(s.requestId, s.attemptNumber));
  const [reviewed, setReviewed] = createSignal<MrpReview | null>(null);
  const [reason, setReason] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  // Reading `data()` of a failed load throws, so every read goes through the error first.
  const response = () => (data.error ? undefined : data());
  const pack = () => response()?.pack;
  const review = () => reviewed() ?? response()?.review ?? null;

  // One `viewed` call for as long as the pack is on screen.
  let viewedSent = false;
  createEffect(() => {
    const r = response();
    const s = which();
    if (!r || !s || viewedSent || r.review?.viewed_at || r.review?.verdict) return;
    viewedSent = true;
    mrpApi.markViewed(s.requestId, s.attemptNumber).catch(() => {
      /* best effort */
    });
  });

  const submit = async (verdict: 'accept' | 'reject') => {
    const s = which();
    if (!s || !reason().trim() || busy()) return;
    setBusy(true);
    try {
      setReviewed(await mrpApi.review(s.requestId, s.attemptNumber, verdict, reason().trim()));
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

  return { data, enabled: () => which() !== undefined, pack, review, reason, setReason, busy, submit };
}

export type Mrp = ReturnType<typeof createMrp>;

/** The verifier's recommendation and risk as two chips, with its one sentence of why. */
export const MrpVerdict: Component<{ pack: MrpPack }> = (props) => (
  <div data-testid="mrp-verdict" class="space-y-1">
    <div class="flex flex-wrap items-center gap-2">
      <Badge tone={props.pack.recommendation.decision === 'merge' ? 'success' : props.pack.recommendation.decision === 'reject' ? 'danger' : 'warning'}>
        Recommendation: {props.pack.recommendation.decision}
      </Badge>
      <Badge tone={RISK_TONE[props.pack.risk.tier] ?? 'neutral'}>Risk: {props.pack.risk.tier}</Badge>
    </div>
    <p class="text-sm" title={plainLine(props.pack.recommendation.rationale).raw} style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>
      <Show when={props.pack.recommendation.decision === 'reject'}>
        <strong>The verifier recommends rejecting this run: </strong>
      </Show>
      {plainLine(props.pack.recommendation.rationale).text}
    </p>
  </div>
);

/** A next step in a sentence: the verifier's "criterion X failed: kind" line names the check, and its kind stays in the tooltip. */
function nextStepWords(s: { code: string; message?: string | null }): string {
  const failed = s.message ? /^criterion (\S+) failed\b/i.exec(s.message) : null;
  return failed ? `The check “${failed[1]}” failed` : (s.message ?? CODE_WORDS[s.code] ?? s.code);
}

/** What each kind of check does, in words; a kind this build does not know is shown as it came. */
const KIND_WORDS: Record<string, string> = {
  command: 'Runs a command',
  test: 'Runs the tests',
  metric: 'Measures a number',
  file: 'Checks a file exists',
  absent: 'Checks a file is absent',
  manual: 'A person checks it',
};
const SOURCE_WORDS: Record<CheckItem['source'], string> = {
  'Done when': "from the item's Done when",
  Agent: "from the agent's checks",
};
/** The verifier's own notes that are codes, in words. Any other note is already a sentence. */
const NOTE_WORDS: Record<string, string> = {
  needs_human: 'waits for a person to decide',
};

/** One check: the glyph, what it checks, and where it comes from as the quieter line, in words;
 *  the raw kind, source, note and evidence are one closed "How we know" away. */
const CheckRow: Component<{ item: CheckItem }> = (props) => {
  const mark = () => CHECK_MARK[props.item.status];
  const note = () => (props.item.note ? (NOTE_WORDS[props.item.note] ?? props.item.note) : null);
  return (
    <li
      data-testid={props.item.fromPack ? 'mrp-criterion' : 'done-when-check'}
      data-status={props.item.status}
      class="flex min-w-0 items-start gap-2.5"
    >
      <Show
        when={props.item.status === 'manual'}
        fallback={
          <span
            role="img"
            aria-label={mark().label}
            title={mark().label}
            class="w-4 flex-none text-center text-base font-bold leading-snug"
            style={{ color: mark().color }}
          >
            {mark().glyph}
          </span>
        }
      >
        {/* The run-state glyph at text size: the half-circle was a speck next to the tick. */}
        <span class="flex-none pt-0.5">
          <RunState state="needs" docket="Needs a person" size="dot" />
        </span>
      </Show>
      <div class="min-w-0 flex-1 text-sm" style={{ 'overflow-wrap': 'anywhere' }}>
        <p style={{ color: 'var(--color-text-primary)' }}>{props.item.title}</p>
        <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          {KIND_WORDS[props.item.kind] ?? props.item.kind} · {SOURCE_WORDS[props.item.source]}
          <Show when={note()}>{(text) => <> · {text()}</>}</Show>
        </p>
        <details class="text-xs">
          <summary class="cursor-pointer" style={{ color: 'var(--color-text-tertiary)' }}>How we know</summary>
          <p style={{ ...mono, color: 'var(--color-text-tertiary)' }}>
            {props.item.kind} · {props.item.source}
            <Show when={props.item.note}>{(raw) => <> · {raw()}</>}</Show>
          </p>
          <Show when={props.item.evidence}>
            {(ref) => <p style={{ ...mono, color: 'var(--color-text-tertiary)' }}>Evidence: {ref()}</p>}
          </Show>
        </details>
      </div>
    </li>
  );
};

/**
 * What the run was checked against, for the Checks section: the checklist (the verifier's criteria
 * and the item's "Done when" list together), what to do next, the verify command with its output
 * folded, the other analyses the verifier ran, and the person's own verdict on the pack. A section
 * the verifier did not run says "Not run".
 */
const MrpPanel: Component<MrpPanelProps> = (props) => {
  const mrp = () => props.mrp;
  const items = () => buildChecks(mrp().pack(), props.doneWhen);
  return (
    <div class="space-y-4">
      <Show when={mrp().data.loading}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading the pack…
        </p>
      </Show>
      <Show when={mrp().data.error}>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load the pack: {mrp().data.error instanceof Error ? (mrp().data.error as Error).message : 'unknown error'}
        </p>
      </Show>
      <Show when={items().length > 0}>
        <ul data-testid="checks-list" class="space-y-2">
          <For each={items()}>{(item) => <CheckRow item={item} />}</For>
        </ul>
      </Show>
      <Show when={mrp().pack()}>
        {(pack) => (
          <>
            <Show when={pack().risk.reasons.length > 0}>
              <Section title="Why this risk">
                <ul class="list-disc pl-5 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                  <For each={pack().risk.reasons}>{(r) => <li title={plainLine(r).raw}>{plainLine(r).text}</li>}</For>
                </ul>
              </Section>
            </Show>

            <Show when={(pack().next_steps ?? []).length > 0}>
              <Section title="Next steps">
                <ul class="space-y-1 text-xs" style={{ color: 'var(--color-text-primary)' }}>
                  <For each={pack().next_steps}>
                    {(s) => (
                      <li data-testid="mrp-next-step" title={`${s.severity}: ${s.code}${s.message ? ` — ${s.message}` : ''}${s.path ? ` — ${s.path}` : ''}`}>
                        <Badge tone={s.severity === 'error' ? 'danger' : s.severity === 'warning' ? 'warning' : 'neutral'}>
                          {SEVERITY_WORDS[s.severity] ?? s.severity}
                        </Badge>{' '}
                        <span>{nextStepWords(s)}</span>
                        {/* These two codes carry an id in `path`, not a file: the stage is said in words, the
                            criterion's id stays in the tooltip. */}
                        <Show when={s.path && s.code === 'stage_degraded'}>
                          <span> ({s.path!.replace(/_/g, ' ')})</span>
                        </Show>
                        <Show when={s.path && s.code !== 'stage_degraded' && s.code !== 'criterion_manual'}>
                          <span style={mono}> {s.path}{s.line != null ? `:${s.line}` : ''}</span>
                        </Show>
                        <Show when={s.command}>
                          <span class="block" style={mono}>{s.command}</span>
                        </Show>
                      </li>
                    )}
                  </For>
                </ul>
              </Section>
            </Show>

            <Section title="Verify">
              <Show when={pack().verify} fallback={<NotRun />}>
                {(v) => (
                  <details data-testid="mrp-verify" class="space-y-1 text-xs">
                    <summary class="cursor-pointer" style={{ color: 'var(--color-accent-ink)' }}>
                      <span style={mono}>{v().command}</span> · exit code <span style={mono}>{v().exit_code}</span>
                    </summary>
                    <Show when={v().output_tail}>
                      <pre class="mt-1 overflow-x-auto rounded-item p-2" style={{ ...mono, 'background-color': 'var(--color-bg-panel)' }}>
                        {v().output_tail}
                      </pre>
                    </Show>
                  </details>
                )}
              </Show>
            </Section>

            <details data-testid="mrp-more" class="space-y-3">
              <summary class="cursor-pointer text-xs font-semibold" style={{ color: 'var(--color-accent-ink)' }}>
                What else the verifier ran
              </summary>
              <div class="mt-2 space-y-3">
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
              </div>
            </details>

            <Show when={!props.verdictAbove || mrp().review()?.verdict}>
            <Section title="Your review">
              <Show
                when={mrp().review()?.verdict}
                fallback={
                  <form
                    class="space-y-3"
                    onSubmit={(e) => {
                      e.preventDefault();
                    }}
                  >
                    <Field
                      label="Reason"
                      value={mrp().reason()}
                      onInput={(e) => mrp().setReason(e.currentTarget.value)}
                      required
                    />
                    <div class="flex flex-wrap items-center gap-2">
                      <Button type="button" size="sm" disabled={mrp().busy() || !mrp().reason().trim()} onClick={() => mrp().submit('accept')}>
                        Accept
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        disabled={mrp().busy() || !mrp().reason().trim()}
                        onClick={() => mrp().submit('reject')}
                      >
                        Reject
                      </Button>
                      <Show when={!mrp().reason().trim()}>
                        <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                          Write a reason to enable Accept and Reject.
                        </span>
                      </Show>
                    </div>
                  </form>
                }
              >
                <p class="text-sm" data-testid="mrp-review">
                  <Badge tone={mrp().review()?.verdict === 'accept' ? 'success' : 'danger'}>
                    {mrp().review()?.verdict === 'accept' ? 'Accepted' : 'Rejected'}
                  </Badge>{' '}
                  {mrp().review()?.reason}
                </p>
              </Show>
            </Section>
            </Show>
          </>
        )}
      </Show>
    </div>
  );
};

export default MrpPanel;
