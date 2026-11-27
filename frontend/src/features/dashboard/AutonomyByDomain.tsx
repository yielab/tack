import { createResource, For, Show } from 'solid-js';
import { useParams } from '@solidjs/router';
import { api } from '../../shared/api';
import type { AutonomyDomain } from '../../shared/api/autonomy';
import type { components } from '../../shared/api/schema.gen';
import { LoadError, Skeleton } from '../../shared/ui';
import Card from '../../shared/ui/Card';
import { rate, Table } from './QualityFigures';

type CutRow = components['schemas']['CutRow'];

/** What each level means for who merges, in the words the project's automation settings use. */
const LEVEL_SENTENCE: Record<string, string> = {
  L3: 'You merge every change yourself.',
  L4: 'Tack merges changes that stay inside the domains.',
  L5: 'Tack merges any change that passes its checks.',
};

/** The row for the work that did not merge comes from the API in lower case. */
function cutName(name: string): string {
  return name === 'not merged' ? 'Not merged' : name;
}

/** The two cuts the board makes of the work by the kind of task, in the order the part lists them. */
const CUTS = [
  { by: 'domain', title: 'By domain', column: 'Domain' },
  { by: 'level', title: 'By level', column: 'Level' },
] as const;

/**
 * Where each kind of task is accepted alone and where a level should move: the project's level, what each
 * domain merged by itself and what was undone, and how the work went by domain and by level. The rates are
 * the board's own and come from the period's runs; a cut the board does not send is shown disabled.
 */
export default function AutonomyByDomain(props: { since?: string }) {
  const params = useParams();
  const projectId = params.id!;

  const [settings, { refetch: refetchSettings }] = createResource(() => api.autonomy.get(projectId));
  const [metrics, { refetch: refetchMetrics }] = createResource(
    () => props.since ?? 'all',
    (since) => api.metrics.factoryMetrics(projectId, since === 'all' ? undefined : since),
  );

  const domains = (): AutonomyDomain[] => settings()?.domains ?? [];
  const merged = () => domains().reduce((sum, d) => sum + (d.merged ?? 0), 0);
  const undone = () => domains().reduce((sum, d) => sum + (d.undone ?? 0), 0);
  const cutRows = (by: string): CutRow[] | undefined =>
    metrics()?.work_quality.cuts.find((c) => c.by === by)?.rows;

  return (
    <div class="flex flex-col gap-12">
      <Show
        when={!settings.error}
        fallback={<LoadError what="the autonomy settings" error={settings.error} onRetry={() => void refetchSettings()} />}
      >
        <Show when={settings.loading && !settings()}>
          <div class="flex flex-col gap-4" aria-hidden="true">
            <Skeleton height="90px" class="rounded-lg!" />
            <Skeleton height="180px" class="rounded-lg!" />
          </div>
        </Show>

        <Show when={settings()}>
          {(s) => (
            <>
              <section aria-labelledby="autonomy-level-heading" class="flex flex-col gap-4">
                <h2 id="autonomy-level-heading" class="text-section text-content">The project's level</h2>
                <Card class="flex flex-wrap items-baseline gap-x-6 gap-y-2 p-5" data-testid="autonomy-level">
                  <p class="m-0 font-heading text-title text-content">{s().level}</p>
                  <p class="m-0 text-reading text-content">{LEVEL_SENTENCE[s().level] ?? ''}</p>
                  <p class="m-0 w-full text-small text-content-muted" data-testid="autonomy-totals">
                    {merged()} merged by itself, {undone()} undone
                  </p>
                </Card>
              </section>

              <section aria-labelledby="autonomy-domains-heading" class="flex flex-col gap-4">
                <h2 id="autonomy-domains-heading" class="text-section text-content">Domains</h2>
                <Card class="p-5">
                  <Show
                    when={domains().length > 0}
                    fallback={<p class="m-0 text-small text-content-muted">No domains are set in this project.</p>}
                  >
                    <Table head={['Domain', 'Level', 'Merged by itself', 'Undone']}>
                      <For each={domains()}>
                        {(domain) => (
                          <tr data-testid={`domain-${domain.name}`}>
                            <td class="text-content">{domain.name}</td>
                            <td>{domain.level}</td>
                            <td>{domain.merged ?? 0}</td>
                            <td>{domain.undone ?? 0}</td>
                          </tr>
                        )}
                      </For>
                    </Table>
                  </Show>
                </Card>
              </section>
            </>
          )}
        </Show>
      </Show>

      <section aria-labelledby="autonomy-rates-heading" class="flex flex-col gap-4">
        <h2 id="autonomy-rates-heading" class="text-section text-content">How the work went</h2>
        <Show
          when={!metrics.error}
          fallback={<LoadError what="the rates by domain and level" error={metrics.error} onRetry={() => void refetchMetrics()} />}
        >
          <Show when={metrics.loading && !metrics()}>
            <Skeleton height="120px" class="rounded-lg!" />
          </Show>
          <Show when={metrics()}>
            <For each={CUTS}>
              {(cut) => (
                <Card class="flex flex-col gap-3 p-5" data-testid={`autonomy-cut-${cut.by}`}>
                  <h3 class="m-0 text-lead text-content">{cut.title}</h3>
                  <Show
                    when={(cutRows(cut.by) ?? []).length > 0 ? cutRows(cut.by) : undefined}
                    fallback={
                      <div role="group" aria-label={cut.title} aria-disabled="true" class="text-content-subtle">
                        <Table head={[cut.column, 'Runs', 'Success', 'First pass', 'Reverted']}>
                          <tr>
                            <td colSpan={5}>Not measured yet: the board does not cut the work by {cut.by}</td>
                          </tr>
                        </Table>
                      </div>
                    }
                  >
                    {(rows) => (
                      <Table head={[cut.column, 'Runs', 'Success', 'First pass', 'Reverted']}>
                        <For each={rows()}>
                          {(row) => (
                            <tr>
                              <td class="text-content">{cutName(row.name)}</td>
                              <td>{row.attempts}</td>
                              <td>{rate(row.success_rate)}</td>
                              <td>{rate(row.first_pass_acceptance)}</td>
                              <td>{rate(row.revert_after_merge)}</td>
                            </tr>
                          )}
                        </For>
                      </Table>
                    )}
                  </Show>
                </Card>
              )}
            </For>
          </Show>
        </Show>
      </section>
    </div>
  );
}
