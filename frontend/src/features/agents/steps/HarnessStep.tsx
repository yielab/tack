import { type Component, type JSX, For, Show } from 'solid-js';
import { Badge, Button } from '../../../shared/ui';
import { HARNESS_KINDS } from '../../../shared/runWithAgent/shared';
import type { RunnerSummary } from '../../../shared/execution/api';
import type { HarnessCapability } from '../../../shared/execution/types';
import { findHarness, harnessesOf } from '../runnerObservations';
import { HARNESS_INSTALL_COMMAND } from '../constants';

export interface HarnessStepProps {
  /** This machine's own active runner row, or `null` when agent execution
   *  is off (step 1) — there is nothing to probe without it. */
  thisMachineRunner: RunnerSummary | null;
  /** Restarts the embedded runner to force a fresh probe — the only way
   *  this build can re-probe (`GET /api/runners` itself returns whatever
   *  the runner last reported at its own startup, never re-probes on
   *  read). */
  onRecheck: () => void;
  rechecking: boolean;
}

/** One harness row's status, entirely from the runner's own probe — never
 *  a check mark derived from anything this page assumes. A harness kind
 *  this runner's snapshot never mentions at all is treated the same as an
 *  explicit "not found on PATH": this build's two adapters always probe
 *  both known harnesses, so a missing entry only happens against an older
 *  or fake snapshot that never ran the probe. */
function statusLabel(harness: HarnessCapability | undefined): { text: string; tone: 'success' | 'neutral' | 'warning' } {
  if (harness && harness.probe_error === null && harness.installed_version) {
    return { text: `Installed v${harness.installed_version}`, tone: 'success' };
  }
  if (harness?.probe_error && !/not found on path/i.test(harness.probe_error)) {
    return { text: 'Could not check', tone: 'warning' };
  }
  return { text: 'Not found', tone: 'neutral' };
}

/** The round initial badge each row leads with — decorative, cycled by
 *  position so neighbouring rows never share a fill. Tokens only. */
const INITIAL_BADGE_STYLES: readonly JSX.CSSProperties[] = [
  { background: 'var(--color-primary-600)', color: 'var(--color-on-accent)' },
  { background: 'var(--color-accent2)', color: 'var(--color-bg-app)' },
  { background: 'var(--color-accent-line)', color: 'var(--color-accent-ink)' },
  { background: 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' },
];

const HarnessStep: Component<HarnessStepProps> = (props) => {
  return (
    <section class="flex flex-col gap-2.5 rounded-[28px] bg-panel px-[22px] py-5">
      <h2 class="text-[20px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
        Agents on this machine
      </h2>

      <Show
        when={props.thisMachineRunner}
        fallback={
          <p class="text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
            Turn on agent execution above to see what's installed here.
          </p>
        }
      >
        <div class="flex flex-col gap-2.5">
          <For each={HARNESS_KINDS}>
            {(kind, index) => {
              const harness = () => findHarness(harnessesOf(props.thisMachineRunner), kind.value);
              const status = () => statusLabel(harness());
              return (
                <div class="flex flex-wrap items-center gap-3 rounded-[22px] bg-app px-3.5 py-3 text-sm">
                  <span
                    class="font-heading grid h-10 w-10 flex-none place-items-center rounded-full text-[17px]"
                    style={INITIAL_BADGE_STYLES[index() % INITIAL_BADGE_STYLES.length]}
                    aria-hidden="true"
                  >
                    {kind.label.charAt(0)}
                  </span>
                  <div class="flex min-w-0 flex-col">
                    <span class="text-sm font-bold" style={{ color: 'var(--color-text-primary)' }}>{kind.label}</span>
                    <span class="font-mono text-[11px]" style={{ color: 'var(--color-text-tertiary)' }}>{kind.value}</span>
                  </div>
                  <Badge tone={status().tone}>{status().text}</Badge>
                  <Show when={status().text === 'Not found'}>
                    <code class="rounded-full bg-panel px-2.5 py-1 font-mono text-[11.5px]" style={{ color: 'var(--color-text-secondary)' }}>
                      {HARNESS_INSTALL_COMMAND[kind.value] ?? 'see the vendor\'s own install instructions'}
                    </code>
                  </Show>
                  <Show when={status().text === 'Could not check'}>
                    <span class="basis-full pl-[52px] font-mono text-[11px]" style={{ color: 'var(--color-danger-600)' }}>{harness()?.probe_error}</span>
                  </Show>
                </div>
              );
            }}
          </For>
          <div class="flex flex-wrap items-center gap-2.5">
            <Button variant="secondary" size="sm" loading={props.rechecking} onClick={props.onRecheck}>
              Re-check
            </Button>
            <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
              Re-checking restarts agent execution on this machine to run a fresh check.
            </p>
          </div>
        </div>
      </Show>
    </section>
  );
};

export default HarnessStep;
