import { createResource, For, Show } from 'solid-js';
import { useParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { Skeleton } from '../../shared/ui';
import type { components } from '../../shared/api/schema.gen';
import type { JSX } from 'solid-js';

type FactoryMetrics = components['schemas']['FactoryMetrics'];
type Ratio = components['schemas']['Ratio'];
type HumanMinutes = components['schemas']['HumanMinutes'];
type VerificationTax = components['schemas']['VerificationTax'];
type Outcomes = components['schemas']['Outcomes'];

/** "1 pack", "2 packs". */
function count(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? '' : 's'}`;
}

export default function FactoryMetrics() {
  const params = useParams();
  const projectId = params.id!;

  const [metrics] = createResource(() => api.metrics.factoryMetrics(projectId));

  return (
    <div class="flex flex-col gap-[18px]">
      {/* Header */}
      <div>
        <h1 class="m-0 text-content" style={{ 'font-size': '34px' }}>
          Factory metrics
        </h1>
        <p class="mt-0.5 text-[13px] text-content-muted">
          Agent work, verification, and outcomes
        </p>
      </div>

      {/* Loading */}
      <Show when={metrics.loading && !metrics()}>
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-3.5" aria-hidden="true">
          <For each={[1, 2, 3, 4, 5, 6]}>{() => <Skeleton height="180px" class="rounded-[28px]!" />}</For>
        </div>
      </Show>

      {/* Metrics cards */}
      <Show when={!metrics.loading && metrics()}>
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-3.5">
          <MetricCard title="Escalation rate" data={metrics()!}>
            {(m) => <RatioCard ratio={m.escalation_rate} details={`${count(m.decisions, 'decision')} / ${count(m.attempts, 'attempt')}`} />}
          </MetricCard>

          <MetricCard title="Human minutes per decision" data={metrics()!}>
            {(m) => <HumanMinutesCard humanMinutes={m.human_minutes_per_decision} />}
          </MetricCard>

          <MetricCard title="Human minutes per pack review" data={metrics()!}>
            {(m) => <HumanMinutesCard humanMinutes={m.human_minutes_per_mrp} />}
          </MetricCard>

          <MetricCard title="Pack acceptance rate" data={metrics()!}>
            {(m) => <RatioCard ratio={m.mrp_acceptance_rate} details={`${m.mrp_acceptance_rate.numerator} accepted / ${m.mrp_acceptance_rate.denominator} reviewed`} subtext={`${m.mrp_unreviewed} unreviewed, ${m.mrp_produced} produced`} />}
          </MetricCard>

          <MetricCard title="Verification tax (tokens)" data={metrics()!}>
            {(m) => <VerificationTaxCard verificationTax={m.verification_tax} />}
          </MetricCard>

          <MetricCard title="Outcomes" data={metrics()!}>
            {(m) => <OutcomesCard outcomes={m.outcomes} />}
          </MetricCard>
        </div>
      </Show>
    </div>
  );
}

function MetricCard<T>(props: { title: string; data: T; children: (data: T) => JSX.Element }) {
  return (
    <div class="rounded-[28px] bg-panel px-5 py-[18px] flex flex-col gap-2.5">
      <h2 class="text-lg text-content">{props.title}</h2>
      {props.children(props.data)}
    </div>
  );
}

function RatioCard(props: { ratio: Ratio; details?: string; subtext?: string }) {
  return (
    <div class="flex flex-col gap-2">
      <Show
        when={props.ratio.value !== null && props.ratio.value !== undefined}
        fallback={
          <div>
            <p class="text-sm text-content">Not measured</p>
            <p class="text-xs text-content-muted">{reasonText(props.ratio.null_reason)}</p>
          </div>
        }
      >
        <div>
          <p class="text-2xl font-heading text-content">
            {(props.ratio.value! * 100).toFixed(1)}%
          </p>
          <Show when={props.details}>
            <p class="text-xs text-content-muted">{props.details}</p>
          </Show>
          <Show when={props.subtext}>
            <p class="text-xs text-content-subtle">{props.subtext}</p>
          </Show>
        </div>
      </Show>
    </div>
  );
}

/** The endpoint's reason for a null, in words. */
function reasonText(reason: string | null | undefined): string {
  if (reason === 'denominator_zero') return 'Nothing to count in this period.';
  if (reason === 'not_measured') return 'Nothing in this period was measured.';
  return reason ?? '';
}

function HumanMinutesCard(props: { humanMinutes: HumanMinutes }) {
  const hm = () => props.humanMinutes;

  return (
    <div class="flex flex-col gap-2">
      <Show
        when={hm().median_minutes !== null && hm().median_minutes !== undefined}
        fallback={
          <div>
            <p class="text-sm text-content">Not measured</p>
            <p class="text-xs text-content-muted">{reasonText(hm().null_reason)}</p>
          </div>
        }
      >
        <div>
          <div class="flex gap-4">
            <div>
              <p class="text-xs text-content-muted">Median</p>
              <p class="text-lg font-heading text-content">{hm().median_minutes!.toFixed(1)} min</p>
            </div>
            <div>
              <p class="text-xs text-content-muted">P90</p>
              <p class="text-lg font-heading text-content">{hm().p90_minutes!.toFixed(1)} min</p>
            </div>
          </div>
          <div class="text-xs text-content-subtle mt-2">
            Timed from when it was opened: {hm().start_viewed_at} · from when it was created (never opened): {hm().start_created_at}
          </div>
          <p class="text-xs text-content-muted mt-1">{hm().measured} resolved</p>
        </div>
      </Show>
    </div>
  );
}

function VerificationTaxCard(props: { verificationTax: VerificationTax }) {
  const vt = () => props.verificationTax;

  return (
    <div class="flex flex-col gap-2">
      <Show
        when={vt().ratio.value !== null && vt().ratio.value !== undefined}
        fallback={
          <div>
            <p class="text-sm text-content">Not measured</p>
            <p class="text-xs text-content-muted">{reasonText(vt().ratio.null_reason)}</p>
          </div>
        }
      >
        <div>
          <p class="text-2xl font-heading text-content">{(vt().ratio.value! * 100).toFixed(1)}%</p>
          <div class="text-xs text-content-subtle mt-2 space-y-1">
            <p>Implementation: {count(vt().implementation_tokens, 'token')} / {count(vt().implementation_attempts, 'attempt')}</p>
            <p>Rework: {count(vt().rework_tokens, 'token')} / {count(vt().rework_attempts, 'attempt')}</p>
            <p>Verification: {count(vt().verification_tokens, 'token')} / {count(vt().verification_packs, 'pack')}</p>
          </div>
        </div>
      </Show>
    </div>
  );
}

function OutcomesCard(props: { outcomes: Outcomes }) {
  const oc = () => props.outcomes;

  return (
    <div class="flex flex-col gap-2">
      <div>
        <p class="text-xs text-content-muted">Pull requests opened</p>
        <p class="text-2xl font-heading text-content">{oc().opened}</p>
        <div class="text-xs text-content-subtle mt-2 space-y-1">
          <p>{oc().merged} merged</p>
          <p>{oc().closed} closed</p>
          <p>{oc().reverted} reverted</p>
        </div>
        <Show when={oc().pqc_rate.value !== null && oc().pqc_rate.value !== undefined}>
          <div class="mt-2">
            <p class="text-xs text-content-muted">Quality (merged & not reverted)</p>
            <p class="text-lg font-heading text-content">{(oc().pqc_rate.value! * 100).toFixed(1)}%</p>
            <p class="text-xs text-content-subtle">{oc().pqc} PQC / {oc().opened} opened</p>
          </div>
        </Show>
      </div>
    </div>
  );
}
