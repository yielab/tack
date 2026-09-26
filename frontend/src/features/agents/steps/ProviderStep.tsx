import { type Component, For, Show } from 'solid-js';
import { Badge } from '../../../shared/ui';
import { HARNESS_KINDS } from '../../../shared/runWithAgent/shared';
import type { RunnerSummary } from '../../../shared/execution/api';
import { findHarness, harnessesOf } from '../runnerObservations';
import { CANNOT_OBSERVE_VENDOR_LOGIN, HARNESS_LOGIN_COMMAND } from '../constants';
import ProviderKeyPanel from '../ProviderKeyPanel';

export interface ProviderStepProps {
  thisMachineRunner: RunnerSummary | null;
  /** Whether step 5's test run has, in this browser tab, completed at
   *  least once against the given harness kind. Never persisted — a reload
   *  is honestly "unverified" again, since nothing here reads back a
   *  completed run's history for every harness that ever existed: no
   *  status here is ever derived from file existence, and none of it is
   *  fabricated memory either. */
  verifiedHarnessKinds: ReadonlySet<string>;
}

/**
 * Step 3 — two independent paths to a credentialed agent, side by side.
 * "Use the agent's own login" is per-installed-harness and purely
 * instructional (Tack has no route that reads back a vendor login's
 * result); "Use Vercel AI Gateway" mounts `ProviderKeyPanel` unchanged.
 */
const ProviderStep: Component<ProviderStepProps> = (props) => {
  return (
    <section class="flex flex-col gap-2.5">
      <h2 class="pt-2 text-[20px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
        Provider
      </h2>

      <div class="grid gap-3 md:grid-cols-2">
        <div class="flex flex-col gap-2.5 rounded-[28px] bg-panel px-5 py-[18px]">
          <h3 class="text-[16px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
            Use the agent's own login
          </h3>
          <Show
            when={props.thisMachineRunner}
            fallback={
              <p class="text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
                Turn on agent execution above to see which agents are installed here.
              </p>
            }
          >
            <For each={HARNESS_KINDS}>
              {(kind) => {
                const harness = () => findHarness(harnessesOf(props.thisMachineRunner), kind.value);
                const installed = () => harness()?.probe_error === null && harness()?.installed_version;
                const verified = () => props.verifiedHarnessKinds.has(kind.value);
                return (
                  <Show when={installed()}>
                    <div class="flex flex-col gap-2 text-sm">
                      <div class="flex items-center gap-2">
                        <span class="text-[13px] font-bold" style={{ color: 'var(--color-text-primary)' }}>{kind.label}</span>
                        <Badge class="ml-auto" tone={verified() ? 'success' : 'neutral'}>
                          {verified() ? 'Verified' : 'Present, unverified'}
                        </Badge>
                      </div>
                      <pre
                        class="overflow-x-auto rounded-[14px] bg-app px-3 py-2 font-mono text-xs"
                        style={{ color: 'var(--color-text-primary)' }}
                      >
                        {HARNESS_LOGIN_COMMAND[kind.value] ?? kind.value}
                      </pre>
                      <p class="text-xs italic" style={{ color: 'var(--color-text-secondary)' }}>
                        {CANNOT_OBSERVE_VENDOR_LOGIN}
                      </p>
                    </div>
                  </Show>
                );
              }}
            </For>
          </Show>
        </div>

        <div class="flex flex-col gap-2.5 rounded-[28px] bg-panel px-5 py-[18px]">
          <h3 class="text-[16px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
            Use Vercel AI Gateway
          </h3>
          <ProviderKeyPanel />
          <p class="text-[11.5px]" style={{ color: 'var(--color-text-tertiary)' }}>
            This is this one provider's own catalog — the count above is not the full
            picture of every model this machine can reach.
          </p>
        </div>
      </div>
    </section>
  );
};

export default ProviderStep;
