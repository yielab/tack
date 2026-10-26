import { createResource, For, Show } from 'solid-js';
import { useParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { Skeleton } from '../../shared/ui';
import type { components } from '../../shared/api/schema.gen';
import type { JSX } from 'solid-js';
import RateCardForm from './RateCardForm';

type FactoryMetrics = components['schemas']['FactoryMetrics'];
type Ratio = components['schemas']['Ratio'];
type HumanMinutes = components['schemas']['HumanMinutes'];
type VerificationTax = components['schemas']['VerificationTax'];
type Outcomes = components['schemas']['Outcomes'];
type Quality = components['schemas']['Quality'];
type RepositoryQuality = components['schemas']['RepositoryQuality'];
/** The fields of prill's `metrics/v1` document that this page shows. The schema types it as opaque. */
type PrillMetrics = {
  escalationRate?: number | null;
  agreement?: { decided: number; agreed: number; rate: number | null };
  escapedDefects?: { merged: number; escaped: number; rate: number | null };
  qualified?: number | null;
  qualifiedRate?: number | null;
};

/** "1 pack", "2 packs". */
function count(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? '' : 's'}`;
}

/** A 0–1 fraction as a percentage with one decimal. */
function percent(v: number): string {
  return `${(v * 100).toFixed(1)}%`;
}

/** Every dollar figure is Tack's estimate from the rate card, never a bill. */
function dollars(v: number): string {
  return `≈ $${v.toFixed(2)} (estimated)`;
}

export default function FactoryMetrics() {
  const params = useParams();
  const projectId = params.id!;

  const [metrics, { refetch }] = createResource(() => api.metrics.factoryMetrics(projectId));

  return (
    <div class="flex flex-col gap-[18px]">
      {/* Header */}
      <div>
        <h1 class="m-0 text-content" style={{ 'font-size': '2.125rem' }}>
          Factory metrics
        </h1>
        <p class="mt-0.5 text-[0.8125rem] text-content-muted">
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

          <MetricCard title="Decided alone" data={metrics()!}>
            {(m) => (
              <RatioCard
                ratio={m.decided_alone_rate}
                details={`${count(m.decided_alone, 'attempt')} / ${count(m.attempts, 'attempt')}`}
                subtext="Escalations the agent's own policy settled; no person was asked, and none is in the figures above."
              />
            )}
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

          <MetricCard title="Cost" data={metrics()!}>
            {(m) => <CostCard costUsd={m.cost_usd} unpricedAttempts={m.cost_unpriced_attempts} asOf={m.rate_card_as_of} pricedModels={m.priced_models} models={m.models} />}
          </MetricCard>

          <MetricCard title={qualityTitle(metrics()!.quality)} data={metrics()!}>
            {(m) => <QualityCard quality={m.quality} unavailable={m.quality_unavailable} />}
          </MetricCard>
        </div>
      </Show>

      <Show when={!metrics.loading && metrics()}>
        <CostTable
          title="Cost by model"
          label="Model"
          rows={metrics()!.cost_by_model.map((r) => ({ ...r, name: r.model ?? 'No model named' }))}
        />
        <Show when={metrics()!.cost_by_team.some((r) => r.team !== 'No team')}>
          <CostTable title="By team" label="Team" rows={metrics()!.cost_by_team.map((r) => ({ ...r, name: r.team }))} />
        </Show>
      </Show>

      <RateCardForm onSaved={() => void refetch()} />
    </div>
  );
}

type CostRow = { name: string; tokens_in: number; cache_read: number; tokens_out: number; attempts: number; usd?: number | null };

/** Counted tokens and attempts per name, the cache reads beside them and not in them; dollars only where the rate card priced the row. */
function CostTable(props: { title: string; label: string; rows: CostRow[] }) {
  return (
    <div class="rounded-[28px] bg-panel px-5 py-[18px] flex flex-col gap-2.5">
      <h2 class="text-lg text-content">{props.title}</h2>
      <table class="w-full text-left text-xs text-content-subtle">
        <thead>
          <tr class="text-content-muted">
            <th class="font-normal">{props.label}</th>
            <th class="font-normal">Tokens in</th>
            <th class="font-normal">Cache reads</th>
            <th class="font-normal">Tokens out</th>
            <th class="font-normal">Attempts</th>
            <th class="font-normal">Cost</th>
          </tr>
        </thead>
        <tbody>
          <For each={props.rows}>
            {(row) => (
              <tr>
                <td class="text-content">{row.name}</td>
                <td>{row.tokens_in}</td>
                <td>{row.cache_read}</td>
                <td>{row.tokens_out}</td>
                <td>{row.attempts}</td>
                <td>{row.usd === null || row.usd === undefined ? 'not priced' : dollars(row.usd)}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
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
        <div class="mt-2">
          <p class="text-xs text-content-muted">Merged and not reverted (Tack's own)</p>
          <p class="text-xs text-content-subtle">{oc().merged_not_reverted} of {oc().opened} opened</p>
        </div>
      </div>
    </div>
  );
}

/** Cost is Tack's: tokens priced by the rate card, plus human minutes at its hour rate. */
function CostCard(props: { costUsd?: number | null; unpricedAttempts?: number | null; asOf?: string | null; pricedModels: number; models: number }) {
  return (
    <div class="flex flex-col gap-2">
      <Show
        when={props.costUsd !== null && props.costUsd !== undefined}
        fallback={
          <div>
            <p class="text-sm text-content">Not measured</p>
            <p class="text-xs text-content-muted">Enter a rate card below to see cost.</p>
          </div>
        }
      >
        <div>
          <p class="text-2xl font-heading text-content">{dollars(props.costUsd!)}</p>
          <div class="text-xs text-content-subtle mt-2 space-y-1">
            <p>{props.pricedModels} of {props.models} models priced</p>
            <Show when={props.asOf}>
              <p>Prices as of {props.asOf}</p>
            </Show>
            <Show when={props.unpricedAttempts}>
              <p>{count(props.unpricedAttempts!, 'attempt')} not priced: the rate card does not list their model</p>
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
}

/** The card's title names prill's version when prill answered. */
function qualityTitle(quality: Quality | null | undefined): string {
  return quality ? `Quality (from prill ${quality.prill_version})` : 'Quality';
}

function QualityCard(props: { quality?: Quality | null; unavailable?: string | null }) {
  return (
    <div class="flex flex-col gap-2">
      <Show
        when={props.quality}
        fallback={<p class="text-xs text-content-muted">{props.unavailable ?? 'Quality is not available.'}</p>}
      >
        {(quality) => (
          <div class="flex flex-col gap-3">
            <For each={quality().repositories}>{(repo) => <RepositoryQualityBlock repo={repo} />}</For>
            <p class="text-xs text-content-subtle">Figures come from prill. Tack shows them and does not compute them.</p>
          </div>
        )}
      </Show>
    </div>
  );
}

function RepositoryQualityBlock(props: { repo: RepositoryQuality }) {
  const m = () => props.repo.metrics as PrillMetrics;

  return (
    <div class="text-xs text-content-subtle space-y-1">
      <p class="text-sm text-content">{props.repo.repository}</p>
      <Show when={m().escalationRate !== null && m().escalationRate !== undefined}>
        <p>Escalation: {percent(m().escalationRate!)}</p>
      </Show>
      <Show when={m().agreement}>
        <p>
          Agreement: {m().agreement!.rate === null ? 'not measured' : percent(m().agreement!.rate!)} ({m().agreement!.agreed} of {m().agreement!.decided} decided)
        </p>
      </Show>
      <Show when={m().escapedDefects}>
        <p>
          Escaped defects: {m().escapedDefects!.escaped} of {m().escapedDefects!.merged} merged
        </p>
      </Show>
      <Show when={m().qualified !== null && m().qualified !== undefined}>
        <p>
          {count(m().qualified!, 'qualified change')}
          <Show when={m().qualifiedRate !== null && m().qualifiedRate !== undefined}>
            {' '}({percent(m().qualifiedRate!)})
          </Show>
        </p>
      </Show>
      <Show when={props.repo.cost_usd !== null && props.repo.cost_usd !== undefined}>
        <p>Cost: {dollars(props.repo.cost_usd!)}</p>
      </Show>
      <Show when={props.repo.qualified_per_usd !== null && props.repo.qualified_per_usd !== undefined}>
        <p>{props.repo.qualified_per_usd!.toFixed(2)} qualified per dollar</p>
      </Show>
    </div>
  );
}
