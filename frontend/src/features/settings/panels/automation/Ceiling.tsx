import { type Accessor, type Component, For, Show } from 'solid-js';
import { HelpHint } from '../../../../shared/ui';
import { HELP } from '../../../../shared/help/texts';
import type { AutonomySettings } from '../../../../shared/api/autonomy';
import { Heading, Note, SECTION } from './parts';

/** The blockers the team lines already say: "The team X runs attended: …". */
const isTeamBlocker = (text: string, names: string[]) => names.some((n) => text.startsWith(`The team ${n} runs attended`));

/** Rung 4, Ceiling: the most autonomy this project can reach, and what holds it down. */
const Ceiling: Component<{ settings: Accessor<AutonomySettings | null | undefined> }> = (props) => {
  const teams = () => props.settings()?.teams ?? [];
  const blockers = () => (props.settings()?.automatic_merge_blocked_by ?? []).filter((b) => !isTeamBlocker(b, teams().map((t) => t.name)));
  return (
    <div class={SECTION} id="automation-ceiling" data-testid="rung-ceiling">
      <div class="flex items-center gap-2">
        <Heading>4. Ceiling</Heading>
        <HelpHint label="Ceiling" help={HELP.ladderCeiling} />
      </div>
      <Show when={props.settings()?.ceiling} fallback={<Note>The ceiling is not available yet.</Note>}>
        {(ceiling) => <p class="text-sm" style={{ color: 'var(--color-text-primary)' }}>This project can reach {ceiling()}.</p>}
      </Show>
      <For each={teams()}>
        {(t) => (
          <p data-testid="ceiling-team" class="text-sm" style={{ color: 'var(--color-text-primary)' }}>
            {t.caps_at ? `✗ The team ${t.name} keeps its tasks at ${t.caps_at}. ${t.fix ?? ''}` : `✓ The team ${t.name} runs lights-out.`}
          </p>
        )}
      </For>
      <For each={blockers()}>
        {(b) => <p data-testid="ceiling-blocker" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>✗ {b}</p>}
      </For>
    </div>
  );
};

export default Ceiling;
