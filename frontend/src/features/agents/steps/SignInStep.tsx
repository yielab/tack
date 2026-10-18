import { type Component, For, Show } from 'solid-js';
import { Badge } from '../../../shared/ui';
import { HARNESS_KINDS } from '../../../shared/runWithAgent/shared';
import type { RunnerSummary } from '../../../shared/execution/api';
import type { HarnessCapability } from '../../../shared/execution/types';
import { agentSafety } from '../../../shared/help/texts';
import { findHarness, harnessesOf, isReady } from '../runnerObservations';

export interface SignInStepProps {
  /** This machine's own active runner row, or `null` when agent execution is off. */
  thisMachineRunner: RunnerSummary | null;
}

/** The agents installed on this computer, in the order the page lists them. */
function installedAgents(runner: RunnerSummary | null): { kind: string; label: string; harness: HarnessCapability }[] {
  const harnesses = harnessesOf(runner);
  return HARNESS_KINDS.flatMap((k) => {
    const harness = findHarness(harnesses, k.value);
    return isReady(harness) ? [{ kind: k.value, label: k.label, harness }] : [];
  });
}

/** True when at least one agent is installed and every installed agent that can run here is signed in. */
export function allSignedIn(runner: RunnerSummary | null): boolean {
  const installed = installedAgents(runner);
  if (installed.length === 0) return false;
  return installed
    .filter(({ harness }) => agentSafety([harness])?.tone !== 'blocked')
    .every(({ harness }) => harness.sign_in?.state === 'signed_in');
}

/**
 * Step 3: one row per installed agent saying whether it is signed in on this computer,
 * from what the runner reports. Tack never reads or stores the agent's credentials.
 */
const SignInStep: Component<SignInStepProps> = (props) => {
  const agents = () => installedAgents(props.thisMachineRunner);
  return (
    <section class="flex flex-col gap-2.5 rounded-[28px] bg-panel px-[22px] py-5">
      <h2 class="text-[1.25rem] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
        Sign in
      </h2>
      <Show
        when={agents().length > 0}
        fallback={
          <p class="text-[0.8125rem]" style={{ color: 'var(--color-text-secondary)' }}>
            Install an agent above, then sign it in here.
          </p>
        }
      >
        <For each={agents()}>
          {({ kind, label, harness }) => {
            const signIn = () => harness.sign_in ?? null;
            const link = () => harness.docs?.sign_in;
            return (
              <div class="flex flex-wrap items-center gap-3 rounded-[22px] bg-app px-3.5 py-3 text-sm" data-testid={`sign-in-${kind}`} data-sign-in={signIn()?.state ?? 'unknown'}>
                <span class="text-sm font-bold" style={{ color: 'var(--color-text-primary)' }}>{label}</span>
                <Show
                  when={signIn()?.state === 'signed_in'}
                  fallback={
                    <Show
                      when={signIn()?.state === 'signed_out'}
                      fallback={<span style={{ color: 'var(--color-text-secondary)' }}>{signIn()?.detail ?? `${label} can't say whether it is signed in.`}</span>}
                    >
                      <span style={{ color: 'var(--color-text-secondary)' }}>
                        Not signed in.
                        <Show when={signIn()?.command}>
                          {(command) => <> Run <code class="font-mono text-[0.75rem]">{command()}</code> in a terminal.</>}
                        </Show>
                      </span>
                    </Show>
                  }
                >
                  <Badge tone="success">Signed in</Badge>
                  <span style={{ color: 'var(--color-text-secondary)' }}>{signIn()?.detail}</span>
                </Show>
                <Show when={link()}>
                  {(href) => (
                    <a href={href()} target="_blank" rel="noopener noreferrer" class="text-xs font-semibold underline" style={{ color: 'var(--color-accent-ink)' }}>
                      How to sign in to {label}
                    </a>
                  )}
                </Show>
              </div>
            );
          }}
        </For>
      </Show>
    </section>
  );
};

export default SignInStep;
