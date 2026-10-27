import { type Component, createSignal, Show } from 'solid-js';
import { IconChevronDown, IconChevronRight } from '../../shared/ui/icons';
import RunnerFleetSection from './runnerFleet/RunnerFleetSection';

/**
 * Everything `RunnerFleetSection.tsx` provides (enrollment, fleets, agent
 * profiles), collapsed by default. The whole `runnerFleet/` directory lives
 * in this feature because `architecture.test.ts` forbids a `features/*`
 * importing another `features/*`.
 * Its own vocabulary ("runner", "fleet", "enroll", "capacity"…) is the
 * vocabulary the numbered steps above deliberately never use, so none of
 * it — including this section's own label — renders while collapsed.
 * Nothing inside `runnerFleet/` is edited here.
 */
const AdvancedSection: Component = () => {
  const [open, setOpen] = createSignal(false);

  return (
    <section id="advanced" class="space-y-4">
      <div class="flex flex-wrap items-center gap-2.5 rounded-full bg-panel px-[22px] py-3.5">
        <button
          type="button"
          class="flex items-center gap-2.5"
          style={{ color: 'var(--color-text-primary)' }}
          onClick={() => setOpen(!open())}
          aria-expanded={open()}
        >
          <Show when={open()} fallback={<IconChevronRight size={14} />}>
            <IconChevronDown size={14} />
          </Show>
          <span class="font-heading text-[1.125rem]">Advanced</span>
        </button>
        {/* Stays unrendered while collapsed: its vocabulary belongs to Advanced only. */}
        <Show when={open()}>
          <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
            Enroll additional machines, manage fleets, and configure agent profiles.
          </p>
        </Show>
      </div>
      <Show when={open()}>
        <RunnerFleetSection />
      </Show>
    </section>
  );
};

export default AdvancedSection;
